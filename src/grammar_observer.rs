//! Compiler witnesses for the reviewed grammar mappings in config/rules.json.
//! Form paths correspond to the extracted grammar tree. Its pinned shape hash
//! prevents these mappings from being reused after a production changes.
use rustc_ast::{
    ast, token,
    visit::{self, Visitor},
};
use rustc_span::{Span, source_map::SourceMap};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PatternResolution {
    Binding,
    Path,
    NotPattern,
    /// The independent source parse has no resolver. Its candidates must match
    /// the selected interpretation from the real compiler before receiving credit.
    SourceOnly,
}

pub fn observe(
    krate: &ast::Crate,
    map: &SourceMap,
    resolve: impl Fn(ast::NodeId) -> PatternResolution,
    emit: impl FnMut(&str, &str, Span),
) {
    Observer {
        map,
        resolve: &resolve,
        non_patterns: Vec::new(),
        non_expressions: Vec::new(),
        emit,
    }
    .visit_crate(krate);
}
struct Observer<'a, F> {
    map: &'a SourceMap,
    resolve: &'a dyn Fn(ast::NodeId) -> PatternResolution,
    non_patterns: Vec<Span>,
    non_expressions: Vec<Span>,
    emit: F,
}
impl<F: FnMut(&str, &str, Span)> Observer<'_, F> {
    fn form(&mut self, name: &str, form: &str, span: Span) {
        if !span.is_dummy() && !span.from_expansion() {
            (self.emit)(name, form, span);
        }
    }
    fn production(&mut self, name: &str, span: Span) {
        self.form(name, "production", span);
    }
    fn choice(&mut self, name: &str, path: &str, alternative: usize, span: Span) {
        self.form(name, &format!("{path}.alternative.{alternative}"), span);
    }
    fn optional(&mut self, name: &str, path: &str, present: bool, span: Span) {
        self.form(
            name,
            &format!("{path}.{}", if present { "present" } else { "absent" }),
            span,
        );
    }
    fn repeated(&mut self, name: &str, path: &str, count: usize, span: Span) {
        self.form(
            name,
            &format!(
                "{path}.{}",
                match count {
                    0 => "zero",
                    1 => "one",
                    _ => "many",
                }
            ),
            span,
        );
    }
    fn operator(&mut self, alternative: usize, span: Span) {
        self.production("OperatorExpression", span);
        self.choice("OperatorExpression", "root", alternative, span);
    }
    fn trailing_comma(&self, whole: Span, last: Span) -> Option<bool> {
        if last.from_expansion() || last.hi() > whole.hi() || last.hi() < whole.lo() {
            return None;
        }
        let suffix = self.map.span_to_snippet(whole.with_lo(last.hi())).ok()?;
        for token in rustc_lexer::tokenize(&suffix, rustc_lexer::FrontmatterAllowed::No) {
            use rustc_lexer::TokenKind::*;
            if matches!(
                token.kind,
                Whitespace | LineComment { .. } | BlockComment { .. }
            ) {
                continue;
            }
            return Some(matches!(token.kind, Comma));
        }
        None
    }
    fn list(&mut self, name: &str, prefix: &str, count: usize, last: Span, span: Span) {
        self.production(name, span);
        self.repeated(name, &format!("{prefix}.sequence.1"), count - 1, span);
        if let Some(trailing) = self.trailing_comma(span, last) {
            self.optional(name, &format!("{prefix}.sequence.2"), trailing, span);
        }
    }
    fn tuple_elements(&mut self, name: &str, prefix: &str, count: usize, last: Span, span: Span) {
        self.production(name, span);
        if let Some(trailing) = self.trailing_comma(span, last) {
            self.repeated(
                name,
                &format!("{prefix}.sequence.0"),
                if trailing { count } else { count - 1 },
                span,
            );
            self.optional(name, &format!("{prefix}.sequence.1"), !trailing, span);
        }
    }
    fn loop_expression(&mut self, alternative: usize, label: &Option<ast::Label>, span: Span) {
        self.production("LoopExpression", span);
        self.optional("LoopExpression", "root.sequence.0", label.is_some(), span);
        self.choice("LoopExpression", "root.sequence.1", alternative, span);
        if label.is_some() {
            self.production("LoopLabel", span);
        }
    }

    fn maybe_named_parameter(&mut self, param: &ast::Param) {
        let named = match &param.pat.kind {
            ast::PatKind::Missing => None,
            ast::PatKind::Ident(_, _, None) => Some(0),
            ast::PatKind::Wild => Some(1),
            _ => return,
        };
        self.production("MaybeNamedParam", param.span);
        self.repeated(
            "MaybeNamedParam",
            "root.sequence.0",
            param.attrs.len(),
            param.span,
        );
        self.optional(
            "MaybeNamedParam",
            "root.sequence.1",
            named.is_some(),
            param.span,
        );
        if let Some(alternative) = named {
            self.choice(
                "MaybeNamedParam",
                "root.sequence.1.body.sequence.0",
                alternative,
                param.span,
            );
        }
    }

    fn function_pointer(&mut self, pointer: &ast::FnPtrTy, span: Span) {
        // The Reference's qualifier grammar has no explicit `safe` alternative.
        let unsafe_ = match pointer.safety {
            ast::Safety::Default => false,
            ast::Safety::Unsafe(_) => true,
            ast::Safety::Safe(_) => return,
        };
        let variadic = pointer
            .decl
            .inputs
            .last()
            .filter(|p| matches!(p.ty.kind, ast::TyKind::CVarArgs));
        // rustc accepts `fn(...)`, but this pinned Reference production requires
        // at least one fixed parameter. Do not credit its parents for that form.
        if variadic.is_some_and(|p| {
            pointer.decl.inputs.len() == 1 || !matches!(p.pat.kind, ast::PatKind::Missing)
        }) {
            return;
        }
        // `for<> fn()` has an empty parameter vector but still has a written binder.
        let binder = span.lo() < pointer.decl_span.lo();
        let explicit_return = matches!(pointer.decl.output, ast::FnRetTy::Ty(_));
        self.production("BareFunctionType", span);
        self.optional("BareFunctionType", "root.sequence.0", binder, span);
        self.optional(
            "BareFunctionType",
            "root.sequence.4",
            !pointer.decl.inputs.is_empty(),
            span,
        );
        self.optional("BareFunctionType", "root.sequence.6", explicit_return, span);
        if binder {
            self.production("ForLifetimes", span);
        }
        if explicit_return {
            self.production("BareFunctionReturnType", span);
        }
        self.production("FunctionTypeQualifiers", span);
        self.optional("FunctionTypeQualifiers", "root.sequence.0", unsafe_, span);
        self.optional(
            "FunctionTypeQualifiers",
            "root.sequence.1",
            !matches!(pointer.ext, ast::Extern::None),
            span,
        );
        match &pointer.ext {
            ast::Extern::None => {}
            ast::Extern::Implicit(_) => self.optional(
                "FunctionTypeQualifiers",
                "root.sequence.1.body.sequence.1",
                false,
                span,
            ),
            ast::Extern::Explicit(abi, _) => {
                self.optional(
                    "FunctionTypeQualifiers",
                    "root.sequence.1.body.sequence.1",
                    true,
                    span,
                );
                self.production("Abi", abi.span);
                self.choice(
                    "Abi",
                    "root",
                    if matches!(abi.style, ast::StrStyle::Cooked) {
                        0
                    } else {
                        1
                    },
                    abi.span,
                );
            }
        }
        if pointer.decl.inputs.is_empty() {
            return;
        }
        self.production("FunctionParametersMaybeNamedVariadic", span);
        self.choice(
            "FunctionParametersMaybeNamedVariadic",
            "root",
            usize::from(variadic.is_some()),
            span,
        );
        if let Some(ellipsis) = variadic {
            let fixed = &pointer.decl.inputs[..pointer.decl.inputs.len() - 1];
            self.production("MaybeNamedFunctionParametersVariadic", span);
            self.repeated(
                "MaybeNamedFunctionParametersVariadic",
                "root.sequence.0",
                fixed.len() - 1,
                span,
            );
            self.repeated(
                "MaybeNamedFunctionParametersVariadic",
                "root.sequence.3",
                ellipsis.attrs.len(),
                span,
            );
            for param in fixed {
                self.maybe_named_parameter(param);
            }
        } else {
            let params = &pointer.decl.inputs;
            self.list(
                "MaybeNamedFunctionParameters",
                "root",
                params.len(),
                params.last().unwrap().span,
                span,
            );
            for param in params {
                self.maybe_named_parameter(param);
            }
        }
    }

    fn struct_expression(&mut self, expr: &ast::StructExpr, span: Span) {
        // The reviewed production uses PathInExpression, not a qualified path.
        if expr.qself.is_some() {
            return;
        }
        let base = match &expr.rest {
            ast::StructRest::Base(_) => true,
            ast::StructRest::None => false,
            _ => return,
        };
        self.production("StructExpression", span);
        self.optional(
            "StructExpression",
            "root.sequence.2",
            base || !expr.fields.is_empty(),
            span,
        );
        if base {
            self.production("StructBase", span);
        }
        if let Some(last) = expr.fields.last() {
            self.choice("StructExpression", "root.sequence.2.body", 0, span);
            self.production("StructExprFields", span);
            self.repeated(
                "StructExprFields",
                "root.sequence.1",
                expr.fields.len() - 1,
                span,
            );
            self.choice(
                "StructExprFields",
                "root.sequence.2",
                usize::from(!base),
                span,
            );
            if !base && let Some(trailing) = self.trailing_comma(span, last.span) {
                self.optional(
                    "StructExprFields",
                    "root.sequence.2.alternative.1",
                    trailing,
                    span,
                );
            }
        } else if base {
            self.choice("StructExpression", "root.sequence.2.body", 1, span);
        }
        for field in &expr.fields {
            self.production("StructExprField", field.span);
            self.repeated(
                "StructExprField",
                "root.sequence.0",
                field.attrs.len(),
                field.span,
            );
            self.choice(
                "StructExprField",
                "root.sequence.1",
                usize::from(!field.is_shorthand),
                field.span,
            );
            if !field.is_shorthand {
                self.choice(
                    "StructExprField",
                    "root.sequence.1.alternative.1.sequence.0",
                    usize::from(
                        field
                            .ident
                            .name
                            .as_str()
                            .bytes()
                            .all(|c| c.is_ascii_digit()),
                    ),
                    field.span,
                );
            }
        }
    }

    fn struct_pattern(&mut self, fields: &[ast::PatField], rest: &ast::PatFieldsRest, span: Span) {
        let rest = match rest {
            ast::PatFieldsRest::Rest(span) => Some(*span),
            ast::PatFieldsRest::None => None,
            _ => return,
        };
        self.production("StructPattern", span);
        self.optional(
            "StructPattern",
            "root.sequence.2",
            rest.is_some() || !fields.is_empty(),
            span,
        );
        if let Some(rest_span) = rest {
            self.production("StructPatternEtCetera", rest_span);
        }
        if let Some(last) = fields.last() {
            self.production("StructPatternElements", span);
            self.choice("StructPatternElements", "root", 0, span);
            if rest.is_some() {
                self.optional(
                    "StructPatternElements",
                    "root.alternative.0.sequence.1",
                    true,
                    span,
                );
                self.choice(
                    "StructPatternElements",
                    "root.alternative.0.sequence.1.body",
                    1,
                    span,
                );
            } else if let Some(trailing) = self.trailing_comma(span, last.span) {
                self.optional(
                    "StructPatternElements",
                    "root.alternative.0.sequence.1",
                    trailing,
                    span,
                );
                if trailing {
                    self.choice(
                        "StructPatternElements",
                        "root.alternative.0.sequence.1.body",
                        0,
                        span,
                    );
                }
            }
            self.production("StructPatternFields", span);
            self.repeated(
                "StructPatternFields",
                "root.sequence.1",
                fields.len() - 1,
                span,
            );
        } else if rest.is_some() {
            self.production("StructPatternElements", span);
            self.choice("StructPatternElements", "root", 1, span);
        }
        for field in fields {
            let modifiers = if field.is_shorthand {
                let ast::PatKind::Ident(mode, _, None) = &field.pat.kind else {
                    continue;
                };
                Some(match *mode {
                    ast::BindingMode::NONE => (false, false),
                    ast::BindingMode::MUT => (false, true),
                    ast::BindingMode::REF => (true, false),
                    ast::BindingMode::REF_MUT => (true, true),
                    _ => continue,
                })
            } else {
                None
            };
            self.production("StructPatternField", field.span);
            self.repeated(
                "StructPatternField",
                "root.sequence.0",
                field.attrs.len(),
                field.span,
            );
            let alternative = if let Some((by_ref, mutable)) = modifiers {
                self.optional(
                    "StructPatternField",
                    "root.sequence.1.alternative.2.sequence.0",
                    by_ref,
                    field.span,
                );
                self.optional(
                    "StructPatternField",
                    "root.sequence.1.alternative.2.sequence.1",
                    mutable,
                    field.span,
                );
                2
            } else if field
                .ident
                .name
                .as_str()
                .bytes()
                .all(|c| c.is_ascii_digit())
            {
                0
            } else {
                1
            };
            self.choice(
                "StructPatternField",
                "root.sequence.1",
                alternative,
                field.span,
            );
        }
    }

    fn literal_pattern(&mut self, expr: &ast::Expr) -> bool {
        let negative = match &expr.kind {
            ast::ExprKind::Lit(_) => false,
            ast::ExprKind::Unary(ast::UnOp::Neg, value)
                if matches!(value.kind, ast::ExprKind::Lit(_)) =>
            {
                true
            }
            _ => return false,
        };
        self.production("LiteralPattern", expr.span);
        self.optional("LiteralPattern", "root.sequence.0", negative, expr.span);
        true
    }

    fn range_bound(&mut self, expr: &ast::Expr) {
        let alternative = if self.literal_pattern(expr) {
            0
        } else if matches!(expr.kind, ast::ExprKind::Path(..)) {
            1
        } else {
            return;
        };
        self.production("RangePatternBound", expr.span);
        self.choice("RangePatternBound", "root", alternative, expr.span);
    }
}

impl<'ast, F: FnMut(&str, &str, Span)> Visitor<'ast> for Observer<'_, F> {
    fn visit_expr(&mut self, expr: &'ast ast::Expr) {
        let span = expr.span;
        if self.non_expressions.contains(&span) {
            visit::walk_expr(self, expr);
            return;
        }
        match &expr.kind {
            ast::ExprKind::AddrOf(..) => self.operator(0, span),
            ast::ExprKind::Unary(ast::UnOp::Deref, _) => {
                self.operator(1, span);
                self.production("DereferenceExpression", span);
            }
            ast::ExprKind::Try(_) => {
                self.operator(2, span);
                self.production("TryPropagationExpression", span);
            }
            ast::ExprKind::Unary(operator, _) => {
                let alternative = match operator {
                    ast::UnOp::Neg => 0,
                    ast::UnOp::Not => 1,
                    _ => unreachable!(),
                };
                self.operator(3, span);
                self.production("NegationExpression", span);
                self.choice("NegationExpression", "root", alternative, span);
            }
            ast::ExprKind::Binary(operator, ..) => {
                let spelling = operator.node.as_str();
                for (parent, name, operators) in [
                    (
                        4,
                        "ArithmeticOrLogicalExpression",
                        &["+", "-", "*", "/", "%", "&", "|", "^", "<<", ">>"][..],
                    ),
                    (
                        5,
                        "ComparisonExpression",
                        &["==", "!=", ">", "<", ">=", "<="][..],
                    ),
                    (6, "LazyBooleanExpression", &["||", "&&"][..]),
                ] {
                    if let Some(alternative) = operators.iter().position(|op| *op == spelling) {
                        self.operator(parent, span);
                        self.production(name, span);
                        self.choice(name, "root", alternative, span);
                    }
                }
            }
            ast::ExprKind::Cast(..) => {
                self.operator(7, span);
                self.production("TypeCastExpression", span);
            }
            ast::ExprKind::Assign(..) => {
                self.operator(8, span);
                self.production("AssignmentExpression", span);
            }
            ast::ExprKind::AssignOp(operator, ..) => {
                let operators = ["+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>="];
                let alternative = operators
                    .iter()
                    .position(|op| *op == operator.node.as_str())
                    .unwrap();
                self.operator(9, span);
                self.production("CompoundAssignmentExpression", span);
                self.choice("CompoundAssignmentExpression", "root", alternative, span);
            }
            ast::ExprKind::Range(start, end, limits) => {
                let form = match (start.is_some(), end.is_some(), limits) {
                    (true, true, ast::RangeLimits::HalfOpen) => Some((0, "RangeExpr")),
                    (true, false, ast::RangeLimits::HalfOpen) => Some((1, "RangeFromExpr")),
                    (false, true, ast::RangeLimits::HalfOpen) => Some((2, "RangeToExpr")),
                    (false, false, ast::RangeLimits::HalfOpen) => Some((3, "RangeFullExpr")),
                    (true, true, ast::RangeLimits::Closed) => Some((4, "RangeInclusiveExpr")),
                    (false, true, ast::RangeLimits::Closed) => Some((5, "RangeToInclusiveExpr")),
                    _ => None,
                };
                if let Some((alternative, name)) = form {
                    self.production("RangeExpression", span);
                    self.choice("RangeExpression", "root", alternative, span);
                    self.production(name, span);
                }
            }
            ast::ExprKind::Ret(value) => {
                self.production("ReturnExpression", span);
                self.optional("ReturnExpression", "root.sequence.1", value.is_some(), span);
            }
            ast::ExprKind::Break(label, value) => {
                self.production("BreakExpression", span);
                self.optional("BreakExpression", "root.sequence.1", label.is_some(), span);
                self.optional("BreakExpression", "root.sequence.2", value.is_some(), span);
            }
            ast::ExprKind::Continue(label) => {
                self.production("ContinueExpression", span);
                self.optional(
                    "ContinueExpression",
                    "root.sequence.1",
                    label.is_some(),
                    span,
                );
            }
            ast::ExprKind::Loop(_, label, _) => {
                self.loop_expression(0, label, span);
                self.production("InfiniteLoopExpression", span);
            }
            ast::ExprKind::While(_, _, label) => {
                self.loop_expression(1, label, span);
                self.production("PredicateLoopExpression", span);
            }
            ast::ExprKind::ForLoop(loop_) if matches!(loop_.kind, ast::ForLoopKind::For) => {
                self.loop_expression(2, &loop_.label, span)
            }
            ast::ExprKind::Block(_, label @ Some(_)) => {
                self.loop_expression(3, label, span);
                self.production("LabelBlockExpression", span);
            }
            ast::ExprKind::Paren(_) => self.production("GroupedExpression", span),
            ast::ExprKind::Array(elements) => {
                self.production("ArrayExpression", span);
                self.optional(
                    "ArrayExpression",
                    "root.sequence.1",
                    !elements.is_empty(),
                    span,
                );
                if let Some(last) = elements.last() {
                    self.choice("ArrayElements", "root", 0, span);
                    self.list(
                        "ArrayElements",
                        "root.alternative.0",
                        elements.len(),
                        last.span,
                        span,
                    );
                }
            }
            ast::ExprKind::Repeat(..) => {
                self.production("ArrayExpression", span);
                self.optional("ArrayExpression", "root.sequence.1", true, span);
                self.production("ArrayElements", span);
                self.choice("ArrayElements", "root", 1, span);
            }
            ast::ExprKind::Tup(elements) => {
                self.production("TupleExpression", span);
                self.optional(
                    "TupleExpression",
                    "root.sequence.1",
                    !elements.is_empty(),
                    span,
                );
                if let Some(last) = elements.last() {
                    self.tuple_elements("TupleElements", "root", elements.len(), last.span, span);
                }
            }
            ast::ExprKind::Call(_, args) => {
                self.production("CallExpression", span);
                self.optional("CallExpression", "root.sequence.2", !args.is_empty(), span);
                if let Some(last) = args.last() {
                    self.list("CallParams", "root", args.len(), last.span, span);
                }
            }
            ast::ExprKind::MethodCall(call) => {
                self.production("MethodCallExpression", span);
                self.optional(
                    "MethodCallExpression",
                    "root.sequence.4",
                    !call.args.is_empty(),
                    span,
                );
                if let Some(last) = call.args.last() {
                    self.list("CallParams", "root", call.args.len(), last.span, span);
                }
            }
            ast::ExprKind::Field(_, name) => self.production(
                if name.name.as_str().bytes().all(|c| c.is_ascii_digit()) {
                    "TupleIndexingExpression"
                } else {
                    "FieldExpression"
                },
                span,
            ),
            ast::ExprKind::Index(..) => self.production("IndexExpression", span),
            ast::ExprKind::Struct(struct_) => self.struct_expression(struct_, span),
            ast::ExprKind::Lit(literal) => {
                let alternative = match literal.kind {
                    token::LitKind::Char => Some(0),
                    token::LitKind::Str => Some(1),
                    token::LitKind::StrRaw(_) => Some(2),
                    token::LitKind::Byte => Some(3),
                    token::LitKind::ByteStr => Some(4),
                    token::LitKind::ByteStrRaw(_) => Some(5),
                    token::LitKind::CStr => Some(6),
                    token::LitKind::CStrRaw(_) => Some(7),
                    token::LitKind::Integer | token::LitKind::Float => {
                        match ast::LitKind::from_token_lit(*literal) {
                            Ok(ast::LitKind::Int(..)) => Some(8),
                            Ok(ast::LitKind::Float(..)) => Some(9),
                            _ => None,
                        }
                    }
                    token::LitKind::Bool => Some(if literal.symbol.as_str() == "true" {
                        10
                    } else {
                        11
                    }),
                    token::LitKind::Err(_) => None,
                };
                if let Some(alternative) = alternative {
                    self.production("LiteralExpression", span);
                    self.choice("LiteralExpression", "root", alternative, span);
                }
            }
            _ => {}
        }
        visit::walk_expr(self, expr);
    }

    fn visit_pat(&mut self, pat: &'ast ast::Pat) {
        let span = pat.span;
        if self.non_patterns.contains(&span) {
            visit::walk_pat(self, pat);
            return;
        }
        let saved = self.non_expressions.len();
        // A pattern's optional minus is not a NegationExpression. Its child
        // literal is still a LiteralExpression according to the Reference.
        match &pat.kind {
            ast::PatKind::Expr(expr)
                if matches!(expr.kind, ast::ExprKind::Unary(ast::UnOp::Neg, _)) =>
            {
                self.non_expressions.push(expr.span)
            }
            ast::PatKind::Range(start, end, _) => {
                for bound in start.iter().chain(end.iter()) {
                    if matches!(bound.kind, ast::ExprKind::Unary(ast::UnOp::Neg, _)) {
                        self.non_expressions.push(bound.span);
                    }
                }
            }
            _ => {}
        }
        match &pat.kind {
            ast::PatKind::Ident(mode, name, sub) if name.name.as_str() != "self" => {
                let resolution = (self.resolve)(pat.id);
                if matches!(
                    resolution,
                    PatternResolution::Path | PatternResolution::SourceOnly
                ) {
                    self.production("PathPattern", span);
                }
                if matches!(
                    resolution,
                    PatternResolution::Binding | PatternResolution::SourceOnly
                ) {
                    let modifiers = match *mode {
                        ast::BindingMode::NONE => Some((false, false)),
                        ast::BindingMode::MUT => Some((false, true)),
                        ast::BindingMode::REF => Some((true, false)),
                        ast::BindingMode::REF_MUT => Some((true, true)),
                        _ => None,
                    };
                    if let Some((by_ref, mutable)) = modifiers {
                        self.production("IdentifierPattern", span);
                        self.optional("IdentifierPattern", "root.sequence.0", by_ref, span);
                        self.optional("IdentifierPattern", "root.sequence.1", mutable, span);
                        self.optional("IdentifierPattern", "root.sequence.3", sub.is_some(), span);
                    }
                }
            }
            ast::PatKind::Path(..) => self.production("PathPattern", span),
            ast::PatKind::Wild => self.production("WildcardPattern", span),
            ast::PatKind::Expr(expr) => {
                if !self.literal_pattern(expr) && matches!(expr.kind, ast::ExprKind::Path(..)) {
                    self.production("PathPattern", span);
                }
            }
            ast::PatKind::Range(start, end, limits) => {
                let form = match (start.is_some(), end.is_some(), limits.node) {
                    (true, true, ast::RangeEnd::Excluded) => Some((0, "RangeExclusivePattern")),
                    (true, true, ast::RangeEnd::Included(ast::RangeSyntax::DotDotEq)) => {
                        Some((1, "RangeInclusivePattern"))
                    }
                    (true, false, ast::RangeEnd::Excluded) => Some((2, "RangeFromPattern")),
                    (false, true, ast::RangeEnd::Excluded) => Some((3, "RangeToExclusivePattern")),
                    (false, true, ast::RangeEnd::Included(ast::RangeSyntax::DotDotEq)) => {
                        Some((4, "RangeToInclusivePattern"))
                    }
                    _ => None,
                };
                let valid_bounds = start
                    .iter()
                    .chain(end.iter())
                    .all(|bound| match &bound.kind {
                        ast::ExprKind::Lit(_) | ast::ExprKind::Path(..) => true,
                        ast::ExprKind::Unary(ast::UnOp::Neg, value) => {
                            matches!(value.kind, ast::ExprKind::Lit(_))
                        }
                        _ => false,
                    });
                if let Some((alternative, name)) = form
                    && valid_bounds
                {
                    self.production("ModernRangePattern", span);
                    self.choice("ModernRangePattern", "root", alternative, span);
                    self.production(name, span);
                    for bound in start.iter().chain(end.iter()) {
                        self.range_bound(bound);
                    }
                }
            }
            ast::PatKind::Struct(None, _, fields, rest) => self.struct_pattern(fields, rest, span),
            ast::PatKind::TupleStruct(None, _, fields) => {
                self.production("TupleStructPattern", span);
                self.optional(
                    "TupleStructPattern",
                    "root.sequence.2",
                    !fields.is_empty(),
                    span,
                );
                if let Some(last) = fields.last() {
                    self.list("TupleStructItems", "root", fields.len(), last.span, span);
                }
            }
            ast::PatKind::Tuple(fields) => {
                self.production("TuplePattern", span);
                self.optional("TuplePattern", "root.sequence.1", !fields.is_empty(), span);
                if let Some(last) = fields.last() {
                    self.production("TuplePatternItems", span);
                    if fields.len() == 1 {
                        if let Some(trailing) = self.trailing_comma(span, last.span) {
                            if trailing {
                                self.choice("TuplePatternItems", "root", 0, span);
                            } else if matches!(last.kind, ast::PatKind::Rest) {
                                self.choice("TuplePatternItems", "root", 1, span);
                            }
                        }
                    } else {
                        self.choice("TuplePatternItems", "root", 2, span);
                        self.list(
                            "TuplePatternItems",
                            "root.alternative.2",
                            fields.len(),
                            last.span,
                            span,
                        );
                    }
                }
            }
            ast::PatKind::Slice(fields) => {
                self.production("SlicePattern", span);
                self.optional("SlicePattern", "root.sequence.1", !fields.is_empty(), span);
                if let Some(last) = fields.last() {
                    self.list("SlicePatternItems", "root", fields.len(), last.span, span);
                }
            }
            ast::PatKind::Paren(_) => self.production("GroupedPattern", span),
            ast::PatKind::Rest => self.production("RestPattern", span),
            _ => {}
        }
        visit::walk_pat(self, pat);
        self.non_expressions.truncate(saved);
    }

    fn visit_pat_field(&mut self, field: &'ast ast::PatField) {
        // A shorthand field contains an AST binding, but its grammar has an
        // IDENTIFIER token rather than a nested IdentifierPattern production.
        if field.is_shorthand {
            self.non_patterns.push(field.pat.span);
        }
        visit::walk_pat_field(self, field);
        if field.is_shorthand {
            self.non_patterns.pop();
        }
    }
    fn visit_ty(&mut self, ty: &'ast ast::Ty) {
        match &ty.kind {
            ast::TyKind::Array(..) => self.production("ArrayType", ty.span),
            ast::TyKind::Slice(_) => self.production("SliceType", ty.span),
            ast::TyKind::Ptr(pointee) => {
                self.production("RawPointerType", ty.span);
                self.choice(
                    "RawPointerType",
                    "root.sequence.1",
                    if pointee.mutbl == ast::Mutability::Mut {
                        0
                    } else {
                        1
                    },
                    ty.span,
                );
            }
            ast::TyKind::Ref(lifetime, pointee)
                if !matches!(pointee.ty.kind, ast::TyKind::ImplicitSelf) =>
            {
                self.production("ReferenceType", ty.span);
                self.optional(
                    "ReferenceType",
                    "root.sequence.1",
                    lifetime.is_some(),
                    ty.span,
                );
                self.optional(
                    "ReferenceType",
                    "root.sequence.2",
                    pointee.mutbl == ast::Mutability::Mut,
                    ty.span,
                );
            }
            ast::TyKind::Never if self.map.span_to_snippet(ty.span).is_ok_and(|s| s == "!") => {
                self.production("NeverType", ty.span)
            }
            ast::TyKind::Infer if self.map.span_to_snippet(ty.span).is_ok_and(|s| s == "_") => {
                self.production("InferredType", ty.span)
            }
            ast::TyKind::Paren(_) => self.production("ParenthesizedType", ty.span),
            ast::TyKind::FnPtr(pointer) => self.function_pointer(pointer, ty.span),
            _ => {}
        }
        if let ast::TyKind::Tup(elements) = &ty.kind {
            self.production("TupleType", ty.span);
            self.choice(
                "TupleType",
                "root",
                if elements.is_empty() { 0 } else { 1 },
                ty.span,
            );
            if let Some(last) = elements.last()
                && let Some(trailing) = self.trailing_comma(ty.span, last.span)
            {
                self.repeated(
                    "TupleType",
                    "root.alternative.1.sequence.1",
                    if trailing {
                        elements.len()
                    } else {
                        elements.len() - 1
                    },
                    ty.span,
                );
                self.optional(
                    "TupleType",
                    "root.alternative.1.sequence.2",
                    !trailing,
                    ty.span,
                );
            }
        }
        let saved = self.non_patterns.len();
        if let ast::TyKind::FnPtr(pointer) = &ty.kind {
            // Names and `_:` in a pointer signature are documentary parameters,
            // not binding or wildcard patterns. Still walk their actual types.
            self.non_patterns
                .extend(pointer.decl.inputs.iter().map(|p| p.pat.span));
        }
        visit::walk_ty(self, ty);
        self.non_patterns.truncate(saved);
    }
}
