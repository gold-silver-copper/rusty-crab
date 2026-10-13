//! Interpreted witnesses for accepted gates whose contracts describe source syntax.
//! These predicates run on the compiler's active AST, never generator annotations.
//! All required forms are specified separately in config/rules.json. They do not
//! claim to enumerate arbitrary combinations or prove runtime behavior.
use crate::grammar_observer::PatternResolution;
use rustc_ast::{
    ast,
    visit::{self, Visitor},
};
use rustc_span::Span;

pub fn observe(
    krate: &ast::Crate,
    resolve: &dyn Fn(ast::NodeId) -> PatternResolution,
    emit: impl FnMut(&str, &str, Span),
) {
    FeatureObserver {
        emit,
        resolve,
        pattern_depth: 0,
        break_targets: Vec::new(),
    }
    .visit_crate(krate);
}

struct FeatureObserver<'a, F> {
    emit: F,
    resolve: &'a dyn Fn(ast::NodeId) -> PatternResolution,
    pattern_depth: usize,
    break_targets: Vec<(Option<String>, &'static str, Span)>,
}

fn has_source_let(expr: &ast::Expr) -> bool {
    if expr.span.from_expansion() {
        return false;
    }
    match &expr.kind {
        ast::ExprKind::Let(..) => true,
        ast::ExprKind::Binary(op, left, right) if op.node == ast::BinOpKind::And => {
            !op.span.from_expansion() && (has_source_let(left) || has_source_let(right))
        }
        _ => false,
    }
}

impl<F: FnMut(&str, &str, Span)> FeatureObserver<'_, F> {
    fn found(&mut self, feature: &str, form: &str, span: Span) {
        if !span.from_expansion() && !span.is_dummy() {
            (self.emit)(feature, form, span);
        }
    }

    fn assignee_markers(&mut self, expr: &ast::Expr) {
        if expr.span.from_expansion() {
            return;
        }
        match &expr.kind {
            ast::ExprKind::Underscore => {
                self.found("destructuring_assignment", "discard", expr.span)
            }
            ast::ExprKind::Range(None, None, ast::RangeLimits::HalfOpen) => {
                self.found("destructuring_assignment", "rest", expr.span)
            }
            ast::ExprKind::Paren(inner) => self.assignee_markers(inner),
            ast::ExprKind::Tup(elements)
            | ast::ExprKind::Array(elements)
            | ast::ExprKind::Call(_, elements) => {
                for element in elements {
                    self.assignee_markers(element);
                }
            }
            ast::ExprKind::Struct(value) => {
                if let ast::StructRest::Rest(span) = value.rest {
                    self.found("destructuring_assignment", "rest", span);
                }
                for field in &value.fields {
                    self.assignee_markers(&field.expr);
                }
            }
            // Indices, dereferences, and callees are places/expressions inside
            // the assignee, not recursive destructuring positions.
            _ => {}
        }
    }

    fn condition(&mut self, kind: &str, condition: &ast::Expr) {
        if condition.span.from_expansion() {
            return;
        }
        if let ast::ExprKind::Let(pat, ..) = &condition.kind {
            self.found(
                if kind == "if" { "if_let" } else { "while_let" },
                "condition",
                condition.span,
            );
            if !pat.span.from_expansion() && matches!(pat.kind, ast::PatKind::Or(_)) {
                self.found("if_while_or_patterns", kind, pat.span);
            }
        }
        // Only the condition's && spine constitutes a let-chain. A nested block
        // containing an unrelated if-let is not evidence for this condition.
        if let ast::ExprKind::Binary(op, ..) = &condition.kind
            && op.node == ast::BinOpKind::And
            && !op.span.from_expansion()
            && has_source_let(condition)
        {
            self.found("let_chains", kind, condition.span);
        }
    }
}

impl<'ast, F: FnMut(&str, &str, Span)> Visitor<'ast> for FeatureObserver<'_, F> {
    fn visit_expr(&mut self, expr: &'ast ast::Expr) {
        // A const expression inside a pattern path may itself contain a match.
        // Its arm patterns start new roots, not subpatterns of the outer path.
        let saved_pattern_depth = self.pattern_depth;
        self.pattern_depth = 0;
        let target = match &expr.kind {
            ast::ExprKind::Loop(_, label, _) => Some((label, "loop")),
            ast::ExprKind::While(_, _, label) => Some((label, "while")),
            ast::ExprKind::ForLoop(loop_) => Some((&loop_.label, "for")),
            ast::ExprKind::Block(_, label @ Some(_)) => Some((label, "block")),
            _ => None,
        };
        if let Some((label, kind)) = target {
            self.break_targets.push((
                label.as_ref().map(|l| l.ident.name.as_str().to_owned()),
                kind,
                expr.span,
            ));
        }
        match &expr.kind {
            ast::ExprKind::Assign(left, _, _) => {
                self.assignee_markers(left);
                let mut lhs = &**left;
                while let ast::ExprKind::Paren(inner) = &lhs.kind {
                    lhs = inner;
                }
                let form = match lhs.kind {
                    ast::ExprKind::Tup(_) => Some("tuple"),
                    ast::ExprKind::Array(_) => Some("array"),
                    ast::ExprKind::Struct(_) => Some("struct"),
                    ast::ExprKind::Call(..) => Some("constructor"),
                    _ => None,
                };
                if let Some(form) = form {
                    self.found("destructuring_assignment", form, expr.span);
                }
            }
            ast::ExprKind::Break(label, value) => {
                let name = label.as_ref().map(|l| l.ident.name.as_str());
                let target = self
                    .break_targets
                    .iter()
                    .rev()
                    .find(|(target, kind, _)| match name {
                        Some(name) => target.as_deref() == Some(name),
                        None => *kind != "block",
                    });
                if let Some((_, kind, span)) = target
                    && !span.from_expansion()
                {
                    if *kind == "loop" && value.is_some() {
                        self.found(
                            "loop_break_value",
                            if label.is_some() {
                                "labeled"
                            } else {
                                "unlabeled"
                            },
                            expr.span,
                        );
                    } else if *kind == "block" && label.is_some() {
                        self.found(
                            "label_break_value",
                            if value.is_some() { "value" } else { "unit" },
                            expr.span,
                        );
                    }
                }
            }
            ast::ExprKind::Gen(_, _, ast::GenBlockKind::Async, _) => {
                self.found("async_await", "block", expr.span)
            }
            ast::ExprKind::Await(_, _) => self.found("async_await", "await", expr.span),
            ast::ExprKind::Closure(closure)
                if matches!(
                    closure.coroutine_kind,
                    Some(ast::CoroutineKind::Async { .. })
                ) =>
            {
                self.found("async_closure", "closure", expr.span)
            }
            ast::ExprKind::Try(_) => self.found("question_mark", "postfix", expr.span),
            ast::ExprKind::ConstBlock(_) => self.found("inline_const", "block", expr.span),
            ast::ExprKind::AddrOf(ast::BorrowKind::Raw, mutability, _) => self.found(
                "raw_ref_op",
                if *mutability == ast::Mutability::Mut {
                    "mut"
                } else {
                    "const"
                },
                expr.span,
            ),
            ast::ExprKind::Field(_, field)
                if field.name.as_str().bytes().all(|b| b.is_ascii_digit()) =>
            {
                self.found("tuple_indexing", "numeric_field", expr.span)
            }
            ast::ExprKind::Struct(value) if !expr.span.from_expansion() => {
                for field in &value.fields {
                    if field.is_shorthand {
                        self.found("field_init_shorthand", "field", field.span);
                    }
                    if field
                        .ident
                        .name
                        .as_str()
                        .bytes()
                        .all(|c| c.is_ascii_digit())
                    {
                        self.found("relaxed_adts", "expression", field.span);
                    }
                    if !field.attrs.is_empty() {
                        self.found("struct_field_attributes", "field", field.span);
                    }
                }
            }
            ast::ExprKind::Range(start, Some(_), ast::RangeLimits::Closed) => self.found(
                "inclusive_range_syntax",
                if start.is_some() { "bounded" } else { "upper" },
                expr.span,
            ),
            ast::ExprKind::If(condition, ..) if !expr.span.from_expansion() => {
                self.condition("if", condition)
            }
            ast::ExprKind::While(condition, ..) if !expr.span.from_expansion() => {
                self.condition("while", condition)
            }
            _ => {}
        }
        visit::walk_expr(self, expr);
        if target.is_some() {
            self.break_targets.pop();
        }
        self.pattern_depth = saved_pattern_depth;
    }

    fn visit_local(&mut self, local: &'ast ast::Local) {
        if matches!(local.kind, ast::LocalKind::InitElse(..)) {
            self.found("let_else", "declaration", local.span);
        }
        visit::walk_local(self, local);
    }

    fn visit_pat(&mut self, pat: &'ast ast::Pat) {
        match &pat.kind {
            ast::PatKind::Or(_) if self.pattern_depth > 0 => {
                self.found("or_patterns", "nested", pat.span)
            }
            ast::PatKind::Ident(_, _, Some(sub)) if !pat.span.from_expansion() => {
                let mut binds = false;
                sub.walk(&mut |p| {
                    if !p.span.from_expansion()
                        && matches!(p.kind, ast::PatKind::Ident(..))
                        && matches!(
                            (self.resolve)(p.id),
                            PatternResolution::Binding | PatternResolution::SourceOnly
                        )
                    {
                        binds = true;
                    }
                    !p.span.from_expansion()
                });
                if binds {
                    self.found("bindings_after_at", "subpattern_binding", pat.span);
                }
            }
            ast::PatKind::Slice(elements) if !pat.span.from_expansion() => {
                for p in elements {
                    match &p.kind {
                        ast::PatKind::Rest => self.found("slice_patterns", "unbound", p.span),
                        ast::PatKind::Ident(_, _, Some(rest))
                            if !rest.span.from_expansion()
                                && matches!(rest.kind, ast::PatKind::Rest) =>
                        {
                            self.found("slice_patterns", "bound", p.span)
                        }
                        _ => {}
                    }
                }
            }
            ast::PatKind::Struct(_, _, fields, _) if !pat.span.from_expansion() => {
                for field in fields {
                    if field
                        .ident
                        .name
                        .as_str()
                        .bytes()
                        .all(|c| c.is_ascii_digit())
                    {
                        self.found("relaxed_adts", "pattern", field.span);
                    }
                }
            }
            ast::PatKind::Paren(_) => self.found("pattern_parentheses", "group", pat.span),
            ast::PatKind::Range(start, Some(_), end) if !end.span.from_expansion() => {
                match (start.is_some(), end.node) {
                    (true, ast::RangeEnd::Excluded) => {
                        self.found("exclusive_range_pattern", "bounded", pat.span)
                    }
                    (true, ast::RangeEnd::Included(ast::RangeSyntax::DotDotEq)) => {
                        self.found("dotdoteq_in_patterns", "bounded", pat.span)
                    }
                    (false, ast::RangeEnd::Included(ast::RangeSyntax::DotDotEq)) => {
                        self.found("half_open_range_patterns", "inclusive_upper", pat.span)
                    }
                    _ => {}
                }
            }
            ast::PatKind::Tuple(elements) | ast::PatKind::TupleStruct(_, _, elements) => {
                if !pat.span.from_expansion() {
                    for element in elements {
                        if matches!(element.kind, ast::PatKind::Rest) {
                            self.found(
                                "dotdot_in_tuple_patterns",
                                if matches!(pat.kind, ast::PatKind::Tuple(_)) {
                                    "tuple"
                                } else {
                                    "tuple_struct"
                                },
                                element.span,
                            );
                        }
                    }
                }
            }
            _ => {}
        }
        self.pattern_depth += 1;
        visit::walk_pat(self, pat);
        self.pattern_depth -= 1;
    }

    fn visit_arm(&mut self, arm: &'ast ast::Arm) {
        if !arm.span.from_expansion()
            && let Some(guard) = &arm.guard
            && has_source_let(&guard.cond)
        {
            self.found(
                "if_let_guard",
                if matches!(guard.cond.kind, ast::ExprKind::Let(..)) {
                    "simple"
                } else {
                    "chain"
                },
                guard.span(),
            );
        }
        visit::walk_arm(self, arm);
    }

    fn visit_param(&mut self, param: &'ast ast::Param) {
        if !param.attrs.is_empty() {
            self.found("param_attrs", "parameter", param.span);
        }
        visit::walk_param(self, param);
    }

    fn visit_generic_param(&mut self, param: &'ast ast::GenericParam) {
        if !param.attrs.is_empty() {
            let form = match param.kind {
                ast::GenericParamKind::Lifetime => "lifetime",
                ast::GenericParamKind::Type { .. } => "type",
                ast::GenericParamKind::Const { .. } => "const",
            };
            self.found("generic_param_attrs", form, param.span());
        }
        visit::walk_generic_param(self, param);
    }

    fn visit_vis(&mut self, vis: &'ast ast::Visibility) {
        if let ast::VisibilityKind::Restricted {
            path, shorthand, ..
        } = &vis.kind
        {
            let form = if *shorthand {
                path.segments.last().map(|s| s.ident.name.as_str())
            } else {
                Some("in_path")
            };
            if let Some(form @ ("crate" | "self" | "super" | "in_path")) = form {
                self.found("pub_restricted", form, vis.span);
            }
        }
        visit::walk_vis(self, vis);
    }

    fn visit_item(&mut self, item: &'ast ast::Item) {
        if let ast::ItemKind::Fn(function) = &item.kind
            && matches!(
                function.sig.header.coroutine_kind,
                Some(ast::CoroutineKind::Async { .. })
            )
        {
            self.found("async_await", "free_fn", item.span);
        }
        if let ast::ItemKind::Struct(_, _, ast::VariantData::Struct { fields, .. }) = &item.kind
            && fields.is_empty()
        {
            self.found("braced_empty_structs", "struct", item.span);
        }
        visit::walk_item(self, item);
    }

    fn visit_assoc_item(&mut self, item: &'ast ast::AssocItem, context: visit::AssocCtxt) {
        let form = match context {
            visit::AssocCtxt::Trait => "trait",
            visit::AssocCtxt::Impl { of_trait: false } => "inherent",
            visit::AssocCtxt::Impl { of_trait: true } => "trait_impl",
        };
        match &item.kind {
            ast::AssocItemKind::Const(_) => self.found("associated_consts", form, item.span),
            ast::AssocItemKind::Type(alias) => {
                self.found("associated_types", form, item.span);
                if matches!(context, visit::AssocCtxt::Trait) && !item.span.from_expansion() {
                    for parameter in &alias.generics.params {
                        let kind = match parameter.kind {
                            ast::GenericParamKind::Lifetime => "lifetime",
                            ast::GenericParamKind::Type { .. } => "type",
                            ast::GenericParamKind::Const { .. } => "const",
                        };
                        self.found("generic_associated_types", kind, parameter.span());
                    }
                }
            }
            ast::AssocItemKind::Fn(function)
                if matches!(context, visit::AssocCtxt::Impl { of_trait: false })
                    && matches!(
                        function.sig.header.coroutine_kind,
                        Some(ast::CoroutineKind::Async { .. })
                    ) =>
            {
                self.found("async_await", "inherent_fn", item.span)
            }
            _ => {}
        }
        visit::walk_assoc_item(self, item, context);
    }

    fn visit_assoc_item_constraint(&mut self, constraint: &'ast ast::AssocItemConstraint) {
        if matches!(constraint.kind, ast::AssocItemConstraintKind::Bound { .. }) {
            self.found("associated_type_bounds", "bound", constraint.span);
        }
        visit::walk_assoc_item_constraint(self, constraint);
    }

    fn visit_fn_header(&mut self, header: &'ast ast::FnHeader) {
        if let ast::Const::Yes(span) = header.constness {
            self.found("min_const_fn", "definition", span);
            if let Some(extern_span) = header.ext.span()
                && !extern_span.from_expansion()
            {
                self.found(
                    "const_extern_fn",
                    if matches!(header.safety, ast::Safety::Unsafe(_)) {
                        "unsafe"
                    } else {
                        "safe"
                    },
                    span,
                );
            }
        }
        visit::walk_fn_header(self, header);
    }

    fn visit_variant(&mut self, variant: &'ast ast::Variant) {
        if let ast::VariantData::Struct { fields, .. } = &variant.data {
            if fields.is_empty() {
                self.found("braced_empty_structs", "variant", variant.span);
            } else {
                self.found("struct_variant", "named_fields", variant.span);
            }
        }
        visit::walk_variant(self, variant);
    }
}
