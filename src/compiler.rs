use crate::{model::*, *};
use anyhow::{Context, Result, ensure};
use rustc_ast::{
    ast,
    visit::{self, Visitor},
};
use rustc_driver::{Callbacks, Compilation};
use rustc_interface::interface;
use rustc_middle::ty::TyCtxt;
use rustc_span::{Span, source_map::SourceMap};
use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

struct Observer {
    root: PathBuf,
    evidence: Vec<Evidence>,
    accepted: bool,
    inputs: inputs::SharedInputs,
    environment: std::collections::BTreeMap<String, Option<String>>,
}

impl Callbacks for Observer {
    fn config(&mut self, config: &mut interface::Config) {
        config.file_loader = Some(Box::new(inputs::TrackingLoader {
            root: self.root.clone(),
            inputs: self.inputs.clone(),
        }));
    }

    fn after_expansion<'tcx>(
        &mut self,
        compiler: &interface::Compiler,
        tcx: TyCtxt<'tcx>,
    ) -> Compilation {
        let (resolver, krate) = tcx.resolver_for_lowering();
        let resolver = resolver.borrow();
        let krate = krate.borrow();
        let mut visitor = AstObserver {
            root: &self.root,
            map: compiler.sess.source_map(),
            evidence: &mut self.evidence,
            context: krate.spans.inner_span,
            stage: "expanded_ast",
            detector: "rustc_ast_visitor_v1",
        };
        visitor.visit_crate(&krate);
        visitor.interpreted(&krate, Some(&resolver));
        self.environment = compiler
            .sess
            .env_depinfo
            .borrow()
            .iter()
            .map(|(name, value)| (name.as_str().to_string(), value.map(|v| hash(v.as_str()))))
            .collect();
        Compilation::Continue
    }

    fn after_analysis<'tcx>(
        &mut self,
        compiler: &interface::Compiler,
        tcx: TyCtxt<'tcx>,
    ) -> Compilation {
        use rustc_middle::ty::{
            self,
            adjustment::{Adjust, PointerCoercion},
        };
        let mut observer = AstObserver {
            root: &self.root,
            map: compiler.sess.source_map(),
            evidence: &mut self.evidence,
            context: rustc_span::DUMMY_SP,
            stage: "typechecked",
            detector: "rustc_typeck_v1",
        };
        for owner in tcx.hir_body_owners() {
            let checked = tcx.typeck(owner);
            for (local_id, adjustments) in checked.adjustments().items_in_stable_order() {
                let hir_id = rustc_hir::HirId {
                    owner: checked.hir_owner,
                    local_id,
                };
                let span = tcx.hir_span(hir_id);
                let mut source = checked.node_type(hir_id);
                for adjustment in adjustments {
                    match adjustment.kind {
                        Adjust::Pointer(PointerCoercion::ClosureFnPointer(_)) => {
                            observer.emit("semantic.coercion.closure_to_fn", span)
                        }
                        Adjust::Pointer(PointerCoercion::Unsize) => {
                            if let (ty::Ref(_, from, _), ty::Ref(_, to, _)) =
                                (source.kind(), adjustment.target.kind())
                            {
                                if matches!((from.kind(), to.kind()), (ty::Array(..), ty::Slice(_)))
                                {
                                    observer.emit("semantic.coercion.array_to_slice", span);
                                }
                                if !matches!(from.kind(), ty::Dynamic(..))
                                    && matches!(to.kind(), ty::Dynamic(..))
                                {
                                    observer.emit("semantic.coercion.concrete_to_dyn", span);
                                }
                            }
                        }
                        _ => {}
                    }
                    source = adjustment.target;
                }
            }
        }
        reconcile_source_syntax(&self.root, &self.inputs, &mut self.evidence);
        self.accepted = true;
        Compilation::Continue
    }
}

struct AstObserver<'a> {
    root: &'a Path,
    map: &'a SourceMap,
    evidence: &'a mut Vec<Evidence>,
    context: Span,
    stage: &'static str,
    detector: &'static str,
}

impl AstObserver<'_> {
    fn interpreted(
        &mut self,
        krate: &ast::Crate,
        resolver: Option<&rustc_middle::ty::ResolverAstLowering<'_>>,
    ) {
        let map = self.map;
        let resolve = |id| {
            use crate::grammar_observer::PatternResolution;
            use rustc_hir::def::Res;
            match resolver {
                None => PatternResolution::SourceOnly,
                Some(resolver) => {
                    match resolver.partial_res_map.get(&id).and_then(|r| r.full_res()) {
                        Some(Res::Local(_)) => PatternResolution::Binding,
                        Some(Res::Def(..)) => PatternResolution::Path,
                        _ => PatternResolution::NotPattern,
                    }
                }
            }
        };
        let stage = self.stage;
        self.detector = "rustc_feature_syntax_v1";
        crate::features::observe(krate, &resolve, |feature, witness, span| {
            self.stage = if resolver.is_some() && feature == "bindings_after_at" {
                "resolved_ast"
            } else {
                stage
            };
            self.named(&format!("feature.{feature}"), witness, span);
        });
        self.stage = stage;
        self.detector = "rustc_reference_v1";
        crate::grammar_observer::observe(krate, map, resolve, |production, form, span| {
            self.stage = if resolver.is_some()
                && ["IdentifierPattern", "PathPattern"].contains(&production)
            {
                "resolved_ast"
            } else {
                stage
            };
            self.named(&format!("grammar.{production}"), form, span);
        });
        self.stage = stage;
    }
    fn named(&mut self, requirement: &str, witness: &str, span: Span) {
        let before = self.evidence.len();
        self.record(requirement, span);
        if self.evidence.len() > before {
            self.evidence.last_mut().unwrap().witness = Some(witness.into());
        }
    }
    fn emit(&mut self, id: &str, span: Span) {
        self.record(id, span);
        for expansion in span.macro_backtrace() {
            if matches!(
                expansion.kind,
                rustc_span::hygiene::ExpnKind::Macro(rustc_span::hygiene::MacroKind::Bang, _)
            ) && !expansion.call_site.from_expansion()
            {
                let before = self.evidence.len();
                self.record("internal.macro_expansion_call", expansion.call_site);
                if self.evidence.len() > before {
                    self.evidence.last_mut().unwrap().macro_backtrace =
                        vec![expansion.kind.descr()];
                }
            }
        }
    }

    fn record(&mut self, id: &str, span: Span) {
        if span.is_dummy() {
            return;
        }
        // Expanded tokens retain an expansion context. Never credit these to source syntax.
        let expanded = span.from_expansion();
        let shown = if expanded {
            span.source_callsite()
        } else {
            span
        };
        let start = self.map.lookup_char_pos(shown.lo());
        let end = self.map.lookup_char_pos(shown.hi());
        if start.file.start_pos != end.file.start_pos {
            return;
        }
        let rustc_span::FileName::Real(name) = &start.file.name else {
            return;
        };
        let Some(name) = name.local_path() else {
            return;
        };
        let Ok(path) = name.canonicalize() else {
            return;
        };
        let Ok(relative) = path.strip_prefix(self.root) else {
            return;
        };
        self.evidence.push(Evidence {
            requirement: id.into(),
            file: relative.to_string_lossy().replace('\\', "/"),
            start: start.file.original_relative_byte_pos(shown.lo()).0 as usize,
            end: start.file.original_relative_byte_pos(shown.hi()).0 as usize,
            line: start.line,
            column: start.col.0 + 1,
            stage: self.stage.into(),
            detector: self.detector.into(),
            origin: if expanded {
                "macro_expansion"
            } else {
                "source"
            }
            .into(),
            configuration: String::new(),
            macro_backtrace: span.macro_backtrace().map(|e| e.kind.descr()).collect(),
            witness: None,
        });
    }
}

impl<'ast> Visitor<'ast> for AstObserver<'_> {
    fn visit_expr(&mut self, e: &'ast ast::Expr) {
        self.emit(classify_ast_ExprKind(&e.kind), e.span);
        match &e.kind {
            ast::ExprKind::Binary(op, ..) => self.emit(classify_ast_BinOpKind(&op.node), op.span),
            ast::ExprKind::AssignOp(op, ..) => {
                self.emit(classify_ast_AssignOpKind(&op.node), op.span)
            }
            ast::ExprKind::Unary(op, ..) => self.emit(classify_ast_UnOp(op), e.span),
            ast::ExprKind::AddrOf(kind, ..) => self.emit(classify_ast_BorrowKind(kind), e.span),
            ast::ExprKind::Range(_, _, limits) => {
                self.emit(classify_ast_RangeLimits(limits), e.span)
            }
            ast::ExprKind::Gen(_, _, kind, _) => self.emit(classify_ast_GenBlockKind(kind), e.span),
            ast::ExprKind::ForLoop(loop_) => {
                self.emit(classify_ast_ForLoopKind(&loop_.kind), e.span)
            }
            ast::ExprKind::Struct(s) => self.emit(classify_ast_StructRest(&s.rest), e.span),
            ast::ExprKind::Match(_, _, kind) => self.emit(classify_ast_MatchKind(kind), e.span),
            ast::ExprKind::Lit(token) => {
                if let Ok(kind) = ast::LitKind::from_token_lit(*token) {
                    self.literal(&kind, e.span);
                }
            }
            _ => {}
        }
        let previous = self.context;
        self.context = e.span;
        visit::walk_expr(self, e);
        self.context = previous;
    }
    fn visit_pat(&mut self, p: &'ast ast::Pat) {
        self.emit(classify_ast_PatKind(&p.kind), p.span);
        match &p.kind {
            ast::PatKind::Ident(mode, ..) => self.emit(classify_ast_ByRef(&mode.0), p.span),
            ast::PatKind::Struct(_, _, _, rest) => {
                self.emit(classify_ast_PatFieldsRest(rest), p.span)
            }
            ast::PatKind::Range(_, _, end) => {
                self.emit(classify_ast_RangeEnd(&end.node), end.span);
                if let ast::RangeEnd::Included(syntax) = &end.node {
                    self.emit(classify_ast_RangeSyntax(syntax), end.span);
                }
            }
            _ => {}
        }
        visit::walk_pat(self, p);
    }
    fn visit_ty(&mut self, t: &'ast ast::Ty) {
        self.emit(classify_ast_TyKind(&t.kind), t.span);
        if let ast::TyKind::TraitObject(_, syntax) = &t.kind {
            self.emit(classify_ast_TraitObjectSyntax(syntax), t.span);
        }
        visit::walk_ty(self, t);
    }
    fn visit_item(&mut self, i: &'ast ast::Item) {
        self.emit(classify_ast_ItemKind(&i.kind), i.span);
        match &i.kind {
            ast::ItemKind::Mod(_, _, kind) => {
                self.emit(classify_ast_ModKind(kind), i.span);
                if let ast::ModKind::Loaded(_, inline, _) = kind {
                    self.emit(classify_ast_Inline(inline), i.span);
                } else if matches!(kind, ast::ModKind::Unloaded) {
                    // The original `mod name;` becomes Loaded(..., Inline::No, ...)
                    // after module loading; preserve that source-form correspondence.
                    self.emit("ast.Inline.No", i.span);
                }
            }
            ast::ItemKind::Trait(t) => self.emit(classify_ast_IsAuto(&t.is_auto), i.span),
            ast::ItemKind::Impl(im) => {
                if let Some(tr) = &im.of_trait {
                    self.emit(classify_ast_ImplPolarity(&tr.polarity), i.span);
                }
            }
            _ => {}
        }
        let previous = self.context;
        self.context = i.span;
        visit::walk_item(self, i);
        self.context = previous;
    }
    fn visit_stmt(&mut self, s: &'ast ast::Stmt) {
        self.emit(classify_ast_StmtKind(&s.kind), s.span);
        if let ast::StmtKind::MacCall(call) = &s.kind {
            self.emit(classify_ast_MacStmtStyle(&call.style), s.span);
        }
        // parse_stmt_mac assigns NoBraces before converting a semicolon-free
        // parentheses/brackets invocation into an expression statement.
        if let ast::StmtKind::Expr(expr) = &s.kind
            && let ast::ExprKind::MacCall(call) = &expr.kind
            && call.args.delim != rustc_ast::token::Delimiter::Brace
        {
            self.emit("ast.MacStmtStyle.NoBraces", s.span);
        }
        visit::walk_stmt(self, s);
    }
    fn visit_vis(&mut self, v: &'ast ast::Visibility) {
        self.emit(classify_ast_VisibilityKind(&v.kind), v.span);
        visit::walk_vis(self, v);
    }
    fn visit_generic_param(&mut self, p: &'ast ast::GenericParam) {
        self.emit(classify_ast_GenericParamKind(&p.kind), p.span());
        visit::walk_generic_param(self, p);
    }
    fn visit_local(&mut self, l: &'ast ast::Local) {
        self.emit(classify_ast_LocalKind(&l.kind), l.span);
        visit::walk_local(self, l);
    }
    fn visit_assoc_item(&mut self, i: &'ast ast::AssocItem, ctxt: visit::AssocCtxt) {
        self.emit(classify_ast_AssocItemKind(&i.kind), i.span);
        self.emit(classify_ast_Defaultness(&i.kind.defaultness()), i.span);
        let previous = self.context;
        self.context = i.span;
        visit::walk_assoc_item(self, i, ctxt);
        self.context = previous;
    }
    fn visit_foreign_item(&mut self, i: &'ast ast::ForeignItem) {
        self.emit(classify_ast_ForeignItemKind(&i.kind), i.span);
        let previous = self.context;
        self.context = i.span;
        visit::walk_item(self, i);
        self.context = previous;
    }
    fn visit_fn_header(&mut self, h: &'ast ast::FnHeader) {
        let span = h.span().unwrap_or(self.context);
        self.emit(classify_ast_Const(&h.constness), span);
        self.emit(classify_ast_Safety(&h.safety), span);
        self.emit(classify_ast_Extern(&h.ext), span);
        if let Some(c) = &h.coroutine_kind {
            self.emit(classify_ast_CoroutineKind(c), span);
        }
        visit::walk_fn_header(self, h);
    }
    fn visit_use_tree(&mut self, t: &'ast ast::UseTree) {
        self.emit(classify_ast_UseTreeKind(&t.kind), t.span());
        visit::walk_use_tree(self, t);
    }
    fn visit_block(&mut self, b: &'ast ast::Block) {
        self.emit(classify_ast_BlockCheckMode(&b.rules), b.span);
        visit::walk_block(self, b);
    }
    fn visit_variant_data(&mut self, v: &'ast ast::VariantData) {
        self.emit(classify_ast_VariantData(v), self.context);
        visit::walk_variant_data(self, v);
    }
    fn visit_capture_by(&mut self, c: &'ast ast::CaptureBy) {
        self.emit(classify_ast_CaptureBy(c), self.context);
        visit::walk_capture_by(self, c);
    }
    fn visit_generic_arg(&mut self, a: &'ast ast::GenericArg) {
        self.emit(classify_ast_GenericArg(a), a.span());
        visit::walk_generic_arg(self, a);
    }
    fn visit_generic_args(&mut self, a: &'ast ast::GenericArgs) {
        self.emit(classify_ast_GenericArgs(a), a.span());
        if let ast::GenericArgs::AngleBracketed(args) = a {
            for arg in &args.args {
                self.emit(classify_ast_AngleBracketedArg(arg), arg.span());
            }
        }
        visit::walk_generic_args(self, a);
    }
    fn visit_param_bound(&mut self, b: &'ast ast::GenericBound, _ctxt: visit::BoundKind) {
        self.emit(classify_ast_GenericBound(b), b.span());
        if let ast::GenericBound::Trait(tr) = b {
            self.emit(
                classify_ast_BoundAsyncness(&tr.modifiers.asyncness),
                tr.span,
            );
            self.emit(
                classify_ast_BoundConstness(&tr.modifiers.constness),
                tr.span,
            );
            self.emit(classify_ast_BoundPolarity(&tr.modifiers.polarity), tr.span);
        }
        visit::walk_param_bound(self, b);
    }
    fn visit_where_predicate(&mut self, p: &'ast ast::WherePredicate) {
        self.emit(classify_ast_WherePredicateKind(&p.kind), p.span);
        visit::walk_where_predicate(self, p);
    }
    fn visit_assoc_item_constraint(&mut self, c: &'ast ast::AssocItemConstraint) {
        self.emit(classify_ast_AssocItemConstraintKind(&c.kind), c.span);
        if let ast::AssocItemConstraintKind::Equality { term } = &c.kind {
            self.emit(classify_ast_Term(term), c.span);
        }
        visit::walk_assoc_item_constraint(self, c);
    }
    fn visit_fn_ret_ty(&mut self, t: &'ast ast::FnRetTy) {
        self.emit(classify_ast_FnRetTy(t), t.span());
        visit::walk_fn_ret_ty(self, t);
    }
    fn visit_param(&mut self, p: &'ast ast::Param) {
        if let Some(receiver) = p.to_self() {
            self.emit(classify_ast_SelfKind(&receiver.node), receiver.span);
        }
        visit::walk_param(self, p);
    }
    fn visit_attribute(&mut self, a: &'ast ast::Attribute) {
        if matches!(a.kind, ast::AttrKind::Synthetic(_)) {
            return;
        }
        self.emit(classify_ast_AttrKind(&a.kind), a.span);
        self.emit(classify_ast_AttrStyle(&a.style), a.span);
        if let ast::AttrKind::Normal(normal) = &a.kind {
            self.emit(classify_ast_AttrArgs(&normal.item.args), a.span);
            if normal.item.path.segments.len() == 1 {
                self.emit(
                    &format!("attribute.{}", normal.item.path.segments[0].ident.name),
                    a.span,
                );
            }
        }
        if let Some(meta) = a.meta() {
            self.meta(&meta);
        }
        visit::walk_attribute(self, a);
    }
}

/// An unexpanded parse supplies original grammatical context. Macro calls need a
/// matching real expansion. Source-syntax witnesses need a matching original AST
/// form as well as their active expanded form: forwarded spans alone are unsafe.
/// Empty/token-forwarding expansions without retained hygiene remain conservative
/// false negatives; parsed calls alone are deliberately insufficient.
fn reconcile_source_syntax(
    root: &Path,
    inputs: &inputs::SharedInputs,
    evidence: &mut Vec<Evidence>,
) {
    use rustc_span::source_map::FileLoader;
    let calls: Vec<_> = evidence
        .iter()
        .filter(|e| e.requirement == "internal.macro_expansion_call")
        .cloned()
        .collect();
    let files: std::collections::BTreeSet<_> = calls
        .iter()
        .chain(evidence.iter().filter(|e| {
            matches!(
                e.detector.as_str(),
                "rustc_feature_syntax_v1" | "rustc_ast_visitor_v1" | "rustc_reference_v1"
            )
        }))
        .map(|e| e.file.clone())
        .collect();
    let loader = inputs::TrackingLoader {
        root: root.to_path_buf(),
        inputs: inputs.clone(),
    };
    let mut original_syntax = std::collections::BTreeSet::new();
    let syntax_key = |e: &Evidence| {
        (
            e.requirement.clone(),
            e.witness.clone(),
            e.file.clone(),
            e.start,
            e.end,
        )
    };
    for file in files {
        let path = root.join(&file);
        let source = match loader.read_file(&path) {
            Ok(source) => source,
            Err(_) => {
                inputs.lock().unwrap().inconsistent.push(format!(
                    "Cannot reread compiler source for syntax reconciliation: {file}"
                ));
                continue;
            }
        };
        let psess = rustc_session::parse::ParseSess::new();
        let name = psess
            .source_map()
            .path_mapping()
            .to_real_filename(psess.source_map().working_dir(), path.as_path());
        let parsed = rustc_parse::new_parser_from_source_str(
            &psess,
            rustc_span::FileName::Real(name),
            source,
            rustc_parse::lexer::StripTokens::ShebangAndFrontmatter,
        );
        let mut parser = match parsed {
            Ok(parser) => parser,
            Err(errors) => {
                for error in errors {
                    error.cancel();
                }
                continue;
            }
        };
        let krate = match parser.parse_crate_mod() {
            Ok(krate) => krate,
            Err(error) => {
                error.cancel();
                continue;
            }
        };
        if psess.dcx().has_errors().is_some() {
            continue;
        }
        let mut candidates = Vec::new();
        let mut visitor = AstObserver {
            root,
            map: psess.source_map(),
            evidence: &mut candidates,
            context: krate.spans.inner_span,
            stage: "unexpanded_parse",
            detector: "rustc_ast_visitor_v1",
        };
        visitor.visit_crate(&krate);
        visitor.interpreted(&krate, None);
        for mut candidate in candidates {
            original_syntax.insert(syntax_key(&candidate));
            if !(candidate.requirement.ends_with(".MacCall")
                || candidate.requirement.starts_with("ast.MacStmtStyle."))
            {
                continue;
            }
            if let Some(call) = calls.iter().find(|call| {
                call.file == candidate.file
                    && call.start == candidate.start
                    && call.end <= candidate.end
            }) {
                candidate.stage = "compiler_expansion".into();
                candidate.detector = "rustc_macro_v1".into();
                candidate.macro_backtrace.clone_from(&call.macro_backtrace);
                evidence.push(candidate);
            }
        }
    }
    evidence.retain(|e| {
        e.requirement != "internal.macro_expansion_call"
            && (!matches!(
                e.detector.as_str(),
                "rustc_feature_syntax_v1" | "rustc_ast_visitor_v1" | "rustc_reference_v1"
            ) || original_syntax.contains(&syntax_key(e)))
    });
}

impl AstObserver<'_> {
    fn literal(&mut self, kind: &ast::LitKind, span: Span) {
        self.emit(classify_ast_LitKind(kind), span);
        match kind {
            ast::LitKind::Int(_, ty) => self.emit(classify_ast_LitIntType(ty), span),
            ast::LitKind::Float(_, ty) => self.emit(classify_ast_LitFloatType(ty), span),
            ast::LitKind::Str(_, style) => self.emit(classify_ast_StrStyle(style), span),
            _ => {}
        }
    }
    fn meta(&mut self, meta: &ast::MetaItem) {
        self.emit(classify_ast_MetaItemKind(&meta.kind), meta.span);
        if let ast::MetaItemKind::List(items) = &meta.kind {
            for item in items {
                self.emit(classify_ast_MetaItemInner(item), meta.span);
                match item {
                    ast::MetaItemInner::MetaItem(nested) => self.meta(nested),
                    ast::MetaItemInner::Lit(literal) => self.literal(&literal.kind, literal.span),
                }
            }
        }
    }
}

pub fn worker(
    root: &Path,
    source: &Path,
    output: &Path,
    artifact: &Path,
    mode: &str,
    profile: profile::Profile,
) -> Result<ExitCode> {
    ensure!(
        !std::env::var_os("RUSTC_BOOTSTRAP").is_some(),
        "compiler worker must run without RUSTC_BOOTSTRAP"
    );
    let root = root.canonicalize()?;
    let source = source.canonicalize()?;
    ensure!(
        source.starts_with(&root),
        "input outside audited source root"
    );
    let mut args = vec![
        "rustc".into(),
        source.to_string_lossy().into_owned(),
        "--sysroot".into(),
        env!("CRAB_SYSROOT").into(),
        "-o".into(),
        artifact.to_string_lossy().into_owned(),
    ];
    args.extend(profile.flags());
    if mode == "__object_worker" {
        ensure!(
            !profile.runs(),
            "object worker requires a compile-only profile"
        );
        args.push("--emit=obj".into());
    } else if mode != "__link_worker" {
        args.push("--emit=metadata".into());
    } else {
        ensure!(
            profile.runs(),
            "this profile has no linking or runtime contract"
        );
    }
    if mode == "__diagnostic_worker" {
        args.push("--error-format=json".into());
    }
    let mut observer = Observer {
        root,
        evidence: Vec::new(),
        accepted: false,
        inputs: Default::default(),
        environment: Default::default(),
    };
    let status =
        rustc_driver::catch_with_exit_code(|| rustc_driver::run_compiler(&args, &mut observer));
    observer.evidence.sort();
    observer.evidence.dedup();
    write_json(
        output,
        &CompilerObservation {
            schema: OBSERVATION_SCHEMA,
            compiler_commit: COMMIT.into(),
            configuration: audit::configuration(profile)?,
            accepted: observer.accepted && status == ExitCode::SUCCESS,
            inputs: observer.inputs.lock().unwrap().clone(),
            environment: observer.environment,
            evidence: observer.evidence,
        },
    )
    .context("writing compiler observations")?;
    Ok(status)
}
