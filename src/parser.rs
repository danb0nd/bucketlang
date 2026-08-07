use crate::ast::*;
use crate::error::{Error, Result};
use crate::lexer::{Token, TokenKind};
use std::collections::BTreeMap;

pub struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Parser<'a> {
    pub fn new(tokens: &'a [Token]) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn bump(&mut self) -> &Token {
        let t = &self.tokens[self.pos];
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    fn expect(&mut self, kind: TokenKind) -> Result<()> {
        let t = self.peek().clone();
        if t.kind != kind {
            return Err(Error::at(
                "parse",
                t.line,
                t.col,
                format!("expected {kind:?}, found {:?}", t.kind),
            ));
        }
        self.bump();
        Ok(())
    }

    pub fn parse_program(&mut self) -> Result<RawProgram> {
        let mut module = None;
        let mut imports = Vec::new();
        let mut aliases = Vec::new();
        let mut buckets = Vec::new();

        // Leading module / import decls
        while self.peek().kind == TokenKind::Ident {
            match self.peek().text.as_str() {
                "module" => {
                    if module.is_some() {
                        let t = self.peek();
                        return Err(Error::at(
                            "parse",
                            t.line,
                            t.col,
                            "duplicate module declaration",
                        ));
                    }
                    if !aliases.is_empty() || !buckets.is_empty() {
                        let t = self.peek();
                        return Err(Error::at(
                            "parse",
                            t.line,
                            t.col,
                            "module must appear before types and buckets",
                        ));
                    }
                    module = Some(self.parse_module_decl()?);
                }
                "import" => {
                    if !aliases.is_empty() || !buckets.is_empty() {
                        let t = self.peek();
                        return Err(Error::at(
                            "parse",
                            t.line,
                            t.col,
                            "import must appear before types and buckets",
                        ));
                    }
                    imports.push(self.parse_import_decl()?);
                }
                _ => break,
            }
        }

        while self.peek().kind != TokenKind::Eof {
            if self.peek().kind == TokenKind::Ident && self.peek().text == "type" {
                aliases.push(self.parse_type_alias()?);
            } else if self.peek().kind == TokenKind::Ident
                && (self.peek().text == "module" || self.peek().text == "import")
            {
                let t = self.peek();
                return Err(Error::at(
                    "parse",
                    t.line,
                    t.col,
                    "module/import must be at the top of the file",
                ));
            } else {
                buckets.push(self.parse_item()?);
            }
        }
        Ok(RawProgram {
            module,
            imports,
            aliases,
            buckets,
        })
    }

    fn parse_module_decl(&mut self) -> Result<String> {
        let t0 = self.peek().clone();
        self.expect(TokenKind::Ident)?; // module
        if t0.text != "module" {
            return Err(Error::at("parse", t0.line, t0.col, "expected module"));
        }
        self.parse_module_path()
    }

    /// `ident (:: ident)*` joined with `::`.
    fn parse_module_path(&mut self) -> Result<String> {
        let name_tok = self.peek().clone();
        if name_tok.kind != TokenKind::Ident {
            return Err(Error::at(
                "parse",
                name_tok.line,
                name_tok.col,
                "expected module name",
            ));
        }
        let mut path = self.bump().text.clone();
        validate_ident(&path, name_tok.line, name_tok.col)?;
        while self.peek().kind == TokenKind::ColonColon {
            self.bump();
            let seg_tok = self.peek().clone();
            if seg_tok.kind != TokenKind::Ident {
                return Err(Error::at(
                    "parse",
                    seg_tok.line,
                    seg_tok.col,
                    "expected name after '::'",
                ));
            }
            let seg = self.bump().text.clone();
            validate_ident(&seg, seg_tok.line, seg_tok.col)?;
            path.push_str("::");
            path.push_str(&seg);
        }
        Ok(path)
    }

    fn parse_import_decl(&mut self) -> Result<crate::ast::ImportDecl> {
        let t0 = self.peek().clone();
        self.expect(TokenKind::Ident)?; // import
        if t0.text != "import" {
            return Err(Error::at("parse", t0.line, t0.col, "expected import"));
        }
        let path = self.parse_module_path()?;

        let alias = if self.peek().kind == TokenKind::Ident && self.peek().text == "as" {
            self.bump(); // as
            let alias_tok = self.peek().clone();
            if alias_tok.kind != TokenKind::Ident {
                return Err(Error::at(
                    "parse",
                    alias_tok.line,
                    alias_tok.col,
                    "expected alias name after 'as'",
                ));
            }
            let alias = self.bump().text.clone();
            validate_ident(&alias, alias_tok.line, alias_tok.col)?;
            Some(alias)
        } else {
            None
        };

        // With `as`, last segment is the item and the prefix is the module.
        // Without `as`, store full path as module; compile may split item later.
        let (module, item) = if alias.is_some() {
            let segs: Vec<&str> = path.split("::").collect();
            if segs.len() < 2 {
                return Err(Error::at(
                    "parse",
                    t0.line,
                    t0.col,
                    "`as` requires an item path (e.g. import util::double as dbl)",
                ));
            }
            let item = segs[segs.len() - 1].to_string();
            let module = segs[..segs.len() - 1].join("::");
            (module, Some(item))
        } else {
            (path, None)
        };

        Ok(crate::ast::ImportDecl {
            module,
            item,
            alias,
        })
    }

    fn parse_type_alias(&mut self) -> Result<RawTypeAlias> {
        let t0 = self.peek().clone();
        self.expect(TokenKind::Ident)?; // type
        if t0.text != "type" {
            return Err(Error::at("parse", t0.line, t0.col, "expected type"));
        }
        let name_tok = self.peek().clone();
        if name_tok.kind != TokenKind::Ident {
            return Err(Error::at(
                "parse",
                name_tok.line,
                name_tok.col,
                "expected type alias name",
            ));
        }
        let name = self.bump().text.clone();
        validate_ident(&name, name_tok.line, name_tok.col)?;
        match name.as_str() {
            "Num" | "Bool" | "Str" | "List" | "Any" => {
                return Err(Error::at(
                    "parse",
                    name_tok.line,
                    name_tok.col,
                    format!("cannot redefine built-in type {name}"),
                ));
            }
            _ => {}
        }
        let mut params = Vec::new();
        if self.peek().kind == TokenKind::LBracket {
            self.bump();
            loop {
                let ptok = self.peek().clone();
                if ptok.kind != TokenKind::Ident {
                    return Err(Error::at(
                        "parse",
                        ptok.line,
                        ptok.col,
                        "expected type parameter name",
                    ));
                }
                let p = self.bump().text.clone();
                validate_ident(&p, ptok.line, ptok.col)?;
                if params.contains(&p) {
                    return Err(Error::at(
                        "parse",
                        ptok.line,
                        ptok.col,
                        format!("duplicate type parameter {p}"),
                    ));
                }
                params.push(p);
                if self.peek().kind == TokenKind::Comma {
                    self.bump();
                    continue;
                }
                break;
            }
            self.expect(TokenKind::RBracket)?;
        }
        self.expect(TokenKind::Eq)?;
        let ty = self.parse_type_with_params(&params)?;
        Ok(RawTypeAlias { name, params, ty })
    }

    fn parse_item(&mut self) -> Result<RawBucket> {
        let mut tests = Vec::new();
        let mut is_entry = false;

        loop {
            match self.peek().kind {
                TokenKind::AtTest => {
                    self.bump();
                    tests.push(self.parse_test_ann(false)?);
                }
                TokenKind::AtTestError => {
                    self.bump();
                    tests.push(self.parse_test_ann(true)?);
                }
                TokenKind::AtEntry => {
                    self.bump();
                    if is_entry {
                        let t = self.peek();
                        return Err(Error::at(
                            "parse",
                            t.line,
                            t.col,
                            "duplicate @entry before bucket",
                        ));
                    }
                    is_entry = true;
                }
                _ => break,
            }
        }

        let mut bucket = self.parse_bucket()?;
        bucket.is_entry = is_entry || bucket.is_entry;
        bucket.tests = tests;
        Ok(bucket)
    }

    fn parse_test_ann(&mut self, expect_error: bool) -> Result<TestAnn> {
        let (call_target, args) = self.parse_call_head()?;
        let expected = if expect_error {
            None
        } else {
            self.expect(TokenKind::EqEq)?;
            Some(self.parse_expr()?)
        };
        Ok(TestAnn {
            call_target,
            args,
            expected,
            expect_error,
        })
    }

    fn parse_call_head(&mut self) -> Result<(String, Vec<Expr>)> {
        let target = self.parse_path_name()?;
        self.expect(TokenKind::LParen)?;
        let args = self.parse_args()?;
        self.expect(TokenKind::RParen)?;
        Ok((target, args))
    }

    /// `name`, `mod::name`, or `#addr` / `#mod::b…`
    fn parse_path_name(&mut self) -> Result<String> {
        let t = self.peek().clone();
        match t.kind {
            TokenKind::Address => Ok(self.bump().text.clone()),
            TokenKind::Ident => {
                let mut path = self.bump().text.clone();
                while self.peek().kind == TokenKind::ColonColon {
                    self.bump();
                    let nt = self.peek().clone();
                    if nt.kind != TokenKind::Ident {
                        return Err(Error::at(
                            "parse",
                            nt.line,
                            nt.col,
                            "expected name after '::'",
                        ));
                    }
                    path.push_str("::");
                    path.push_str(&self.bump().text);
                }
                Ok(path)
            }
            _ => Err(Error::at(
                "parse",
                t.line,
                t.col,
                "expected name or address",
            )),
        }
    }

    fn parse_bucket(&mut self) -> Result<RawBucket> {
        let t0 = self.peek().clone();
        let explicit_addr = if t0.kind == TokenKind::Address {
            Some(self.bump().text.clone())
        } else {
            None
        };

        let label = if self.peek().kind == TokenKind::Ident {
            let name = self.bump().text.clone();
            validate_ident(&name, t0.line, t0.col)?;
            Some(name)
        } else {
            None
        };

        if explicit_addr.is_none() && label.is_none() {
            return Err(Error::at(
                "parse",
                t0.line,
                t0.col,
                "bucket needs a label or explicit #address",
            ));
        }

        self.expect(TokenKind::LParen)?;
        let params = self.parse_params()?;
        self.expect(TokenKind::RParen)?;
        self.expect(TokenKind::Arrow)?;
        let ret = self.parse_type()?;

        let desc = if self.peek().kind == TokenKind::String {
            Some(self.bump().text.clone())
        } else {
            None
        };

        // Span the body so edits can splice by offset instead of searching text.
        // `{` is one byte, so the body starts immediately after it.
        let open = self.peek().start;
        self.expect(TokenKind::LBrace)?;
        let body = self.parse_block_body()?;
        let close = self.peek().start;
        self.expect(TokenKind::RBrace)?;

        Ok(RawBucket {
            explicit_addr,
            label,
            contract: Contract { params, ret },
            desc,
            body,
            is_entry: false,
            tests: Vec::new(),
            body_span: Some(Span {
                start: open + 1,
                end: close,
            }),
        })
    }

    /// Bindings `name = expr`, side-effect lines like `print(x)`, then a final expression.
    fn parse_block_body(&mut self) -> Result<Expr> {
        let mut stmts = Vec::new();

        loop {
            if self.peek().kind == TokenKind::RBrace {
                let t = self.peek();
                return Err(Error::at(
                    "parse",
                    t.line,
                    t.col,
                    "bucket body needs a final expression",
                ));
            }

            // `name = expr` binding
            if self.peek().kind == TokenKind::Ident {
                let name_tok = self.peek().clone();
                let next = self.tokens.get(self.pos + 1);
                if next.is_some_and(|t| t.kind == TokenKind::Eq) {
                    let name = self.bump().text.clone();
                    validate_ident(&name, name_tok.line, name_tok.col)?;
                    self.expect(TokenKind::Eq)?;
                    let value = self.parse_expr()?;
                    if self.peek().kind == TokenKind::RBrace {
                        // last line is a binding — its value is the result
                        return Ok(if stmts.is_empty() {
                            value
                        } else {
                            Expr::Block {
                                stmts,
                                result: Box::new(value),
                            }
                        });
                    }
                    stmts.push(Stmt::Bind { name, value });
                    continue;
                }
            }

            let expr = self.parse_expr()?;
            if self.peek().kind == TokenKind::RBrace {
                return Ok(if stmts.is_empty() {
                    expr
                } else {
                    Expr::Block {
                        stmts,
                        result: Box::new(expr),
                    }
                });
            }
            // more follows — this expr is a side-effect statement
            stmts.push(Stmt::Run(expr));
        }
    }

    fn parse_params(&mut self) -> Result<Vec<Param>> {
        let mut params = Vec::new();
        if self.peek().kind == TokenKind::RParen {
            return Ok(params);
        }
        loop {
            let name_tok = self.peek().clone();
            if name_tok.kind != TokenKind::Ident {
                return Err(Error::at(
                    "parse",
                    name_tok.line,
                    name_tok.col,
                    "expected parameter name",
                ));
            }
            let name = self.bump().text.clone();
            validate_ident(&name, name_tok.line, name_tok.col)?;
            self.expect(TokenKind::Colon)?;
            let ty = self.parse_type()?;
            params.push(Param { name, ty });
            if self.peek().kind == TokenKind::Comma {
                self.bump();
                continue;
            }
            break;
        }
        Ok(params)
    }

    fn parse_type(&mut self) -> Result<Type> {
        self.parse_type_with_params(&[])
    }

    fn parse_type_with_params(&mut self, params: &[String]) -> Result<Type> {
        let first = self.parse_type_atom(params)?;
        if self.peek().kind != TokenKind::Pipe {
            return Ok(first);
        }
        // Variant union: Tag | Tag(T) | ...
        let mut tags = BTreeMap::new();
        self.push_variant_atom(&mut tags, first)?;
        while self.peek().kind == TokenKind::Pipe {
            self.bump();
            let atom = self.parse_type_atom(params)?;
            self.push_variant_atom(&mut tags, atom)?;
        }
        if tags.len() < 2 {
            return Err(Error::msg(
                "variant type needs at least two tags (e.g. None | Some(Num))",
            ));
        }
        Ok(Type::Variant(tags))
    }

    fn push_variant_atom(
        &self,
        tags: &mut BTreeMap<String, Option<Type>>,
        atom: Type,
    ) -> Result<()> {
        match atom {
            Type::Name(tag) => {
                if tags.insert(tag.clone(), None).is_some() {
                    return Err(Error::msg(format!("duplicate variant tag {tag}")));
                }
                Ok(())
            }
            Type::Variant(partial) if partial.len() == 1 => {
                let (tag, payload) = partial.into_iter().next().unwrap();
                if tags.insert(tag.clone(), payload).is_some() {
                    return Err(Error::msg(format!("duplicate variant tag {tag}")));
                }
                Ok(())
            }
            other => Err(Error::msg(format!(
                "variant tag must look like None or Some(T), got {}",
                other.name()
            ))),
        }
    }

    fn parse_type_atom(&mut self, params: &[String]) -> Result<Type> {
        let t = self.peek().clone();
        match t.kind {
            TokenKind::LBrace => self.parse_record_type(params),
            TokenKind::Ident => {
                let name = self.bump().text.clone();
                match name.as_str() {
                    "Num" => Ok(Type::Num),
                    "Bool" => Ok(Type::Bool),
                    "Str" => Ok(Type::Str),
                    "List" => {
                        self.expect(TokenKind::LBracket)?;
                        let inner = self.parse_type_with_params(params)?;
                        self.expect(TokenKind::RBracket)?;
                        Ok(Type::List(Box::new(inner)))
                    }
                    other => {
                        if params.iter().any(|p| p == other) {
                            return Ok(Type::Param(other.to_string()));
                        }
                        if self.peek().kind == TokenKind::LBracket {
                            self.bump();
                            let mut args = Vec::new();
                            loop {
                                args.push(self.parse_type_with_params(params)?);
                                if self.peek().kind == TokenKind::Comma {
                                    self.bump();
                                    continue;
                                }
                                break;
                            }
                            self.expect(TokenKind::RBracket)?;
                            Ok(Type::App {
                                name: other.to_string(),
                                args,
                            })
                        } else if self.peek().kind == TokenKind::LParen {
                            // Tag(Payload) as a one-tag variant atom for unions
                            self.bump();
                            let payload = self.parse_type_with_params(params)?;
                            self.expect(TokenKind::RParen)?;
                            let mut m = BTreeMap::new();
                            m.insert(other.to_string(), Some(payload));
                            Ok(Type::Variant(m))
                        } else {
                            Ok(Type::Name(other.to_string()))
                        }
                    }
                }
            }
            _ => Err(Error::at("parse", t.line, t.col, "expected type")),
        }
    }

    fn parse_record_type(&mut self, params: &[String]) -> Result<Type> {
        let t0 = self.peek().clone();
        self.expect(TokenKind::LBrace)?;
        let mut fields = BTreeMap::new();
        if self.peek().kind != TokenKind::RBrace {
            loop {
                let name_tok = self.peek().clone();
                if name_tok.kind != TokenKind::Ident {
                    return Err(Error::at(
                        "parse",
                        name_tok.line,
                        name_tok.col,
                        "expected field name in record type",
                    ));
                }
                let name = self.bump().text.clone();
                validate_ident(&name, name_tok.line, name_tok.col)?;
                self.expect(TokenKind::Colon)?;
                let ty = self.parse_type_with_params(params)?;
                if fields.insert(name.clone(), ty).is_some() {
                    return Err(Error::at(
                        "parse",
                        name_tok.line,
                        name_tok.col,
                        format!("duplicate record field {name}"),
                    ));
                }
                if self.peek().kind == TokenKind::Comma {
                    self.bump();
                    continue;
                }
                break;
            }
        }
        self.expect(TokenKind::RBrace)?;
        if fields.is_empty() {
            return Err(Error::at(
                "parse",
                t0.line,
                t0.col,
                "record type needs at least one field",
            ));
        }
        Ok(Type::Record(fields))
    }

    fn parse_expr(&mut self) -> Result<Expr> {
        if self.peek().kind == TokenKind::Ident && self.peek().text == "if" {
            return self.parse_if();
        }
        if self.peek().kind == TokenKind::Ident && self.peek().text == "match" {
            return self.parse_match();
        }
        self.parse_pipe()
    }

    /// `a |> f` → `f(a)`; `a |> f(x)` → `f(a, x)` (thread as first arg).
    fn parse_pipe(&mut self) -> Result<Expr> {
        let mut left = self.parse_or()?;
        while self.peek().kind == TokenKind::PipeGt {
            let t = self.peek().clone();
            self.bump();
            let rhs = self.parse_or()?;
            left = match rhs {
                Expr::Call { target, mut args } => {
                    args.insert(0, left);
                    Expr::Call { target, args }
                }
                Expr::Var(name) => Expr::Call {
                    target: name,
                    args: vec![left],
                },
                _ => {
                    return Err(Error::at(
                        "parse",
                        t.line,
                        t.col,
                        "`|>` right-hand side must be a name or call",
                    ));
                }
            };
        }
        Ok(left)
    }

    fn parse_match(&mut self) -> Result<Expr> {
        let t = self.peek().clone();
        self.expect(TokenKind::Ident)?; // match
        if t.text != "match" {
            return Err(Error::at("parse", t.line, t.col, "expected match"));
        }
        let scrutinee = self.parse_or()?;
        self.expect(TokenKind::LBrace)?;
        let mut arms = Vec::new();
        while self.peek().kind != TokenKind::RBrace {
            let tag_tok = self.peek().clone();
            if tag_tok.kind != TokenKind::Ident {
                return Err(Error::at(
                    "parse",
                    tag_tok.line,
                    tag_tok.col,
                    "expected variant tag in match arm",
                ));
            }
            let tag = self.bump().text.clone();
            let binder = if self.peek().kind == TokenKind::LParen {
                self.bump();
                let bt = self.peek().clone();
                if bt.kind != TokenKind::Ident {
                    return Err(Error::at(
                        "parse",
                        bt.line,
                        bt.col,
                        "expected binder name in match arm",
                    ));
                }
                let name = self.bump().text.clone();
                validate_ident(&name, bt.line, bt.col)?;
                self.expect(TokenKind::RParen)?;
                Some(name)
            } else {
                None
            };
            self.expect(TokenKind::FatArrow)?;
            let body = self.parse_expr()?;
            arms.push(MatchArm { tag, binder, body });
            if self.peek().kind == TokenKind::Comma {
                self.bump();
            }
        }
        self.expect(TokenKind::RBrace)?;
        if arms.is_empty() {
            return Err(Error::at(
                "parse",
                t.line,
                t.col,
                "match needs at least one arm",
            ));
        }
        Ok(Expr::Match {
            scrutinee: Box::new(scrutinee),
            arms,
        })
    }

    fn parse_if(&mut self) -> Result<Expr> {
        let t = self.peek().clone();
        self.expect(TokenKind::Ident)?; // if
        if t.text != "if" {
            return Err(Error::at("parse", t.line, t.col, "expected if"));
        }
        let cond = self.parse_or()?;
        let then_tok = self.peek().clone();
        if then_tok.kind != TokenKind::Ident || then_tok.text != "then" {
            return Err(Error::at(
                "parse",
                then_tok.line,
                then_tok.col,
                "expected then after if condition",
            ));
        }
        self.bump();
        let then_branch = self.parse_expr()?;
        let else_tok = self.peek().clone();
        if else_tok.kind != TokenKind::Ident || else_tok.text != "else" {
            return Err(Error::at(
                "parse",
                else_tok.line,
                else_tok.col,
                "expected else after then branch",
            ));
        }
        self.bump();
        let else_branch = self.parse_expr()?;
        Ok(Expr::If {
            cond: Box::new(cond),
            then_branch: Box::new(then_branch),
            else_branch: Box::new(else_branch),
        })
    }

    fn parse_or(&mut self) -> Result<Expr> {
        let mut left = self.parse_and()?;
        while self.peek().kind == TokenKind::PipePipe {
            self.bump();
            let right = self.parse_and()?;
            left = Expr::Call {
                target: "#c.or".into(),
                args: vec![left, right],
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr> {
        let mut left = self.parse_compare()?;
        while self.peek().kind == TokenKind::AmpAmp {
            self.bump();
            let right = self.parse_compare()?;
            left = Expr::Call {
                target: "#c.and".into(),
                args: vec![left, right],
            };
        }
        Ok(left)
    }

    fn parse_compare(&mut self) -> Result<Expr> {
        let left = self.parse_add()?;
        let op = match self.peek().kind {
            TokenKind::EqEq => Some("#c.eq"),
            TokenKind::BangEq => Some("#c.ne"),
            TokenKind::Lt => Some("#c.lt"),
            TokenKind::Gt => Some("#c.gt"),
            TokenKind::LtEq => Some("#c.le"),
            TokenKind::GtEq => Some("#c.ge"),
            _ => None,
        };
        if let Some(target) = op {
            self.bump();
            let right = self.parse_add()?;
            Ok(Expr::Call {
                target: target.into(),
                args: vec![left, right],
            })
        } else {
            Ok(left)
        }
    }

    fn parse_add(&mut self) -> Result<Expr> {
        let mut left = self.parse_term()?;
        loop {
            match self.peek().kind {
                TokenKind::Plus => {
                    self.bump();
                    let right = self.parse_term()?;
                    left = Expr::Call {
                        target: "#c.add".into(),
                        args: vec![left, right],
                    };
                }
                TokenKind::Minus => {
                    self.bump();
                    let right = self.parse_term()?;
                    left = Expr::Call {
                        target: "#c.sub".into(),
                        args: vec![left, right],
                    };
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_term(&mut self) -> Result<Expr> {
        let mut left = self.parse_power()?;
        loop {
            match self.peek().kind {
                TokenKind::Star => {
                    self.bump();
                    let right = self.parse_power()?;
                    left = Expr::Call {
                        target: "#c.mul".into(),
                        args: vec![left, right],
                    };
                }
                TokenKind::Slash => {
                    self.bump();
                    let right = self.parse_power()?;
                    left = Expr::Call {
                        target: "#c.div".into(),
                        args: vec![left, right],
                    };
                }
                _ => break,
            }
        }
        Ok(left)
    }

    /// Right-associative `**` / pow.
    fn parse_power(&mut self) -> Result<Expr> {
        let left = self.parse_unary()?;
        if self.peek().kind == TokenKind::StarStar {
            self.bump();
            let right = self.parse_power()?;
            Ok(Expr::Call {
                target: "#c.pow".into(),
                args: vec![left, right],
            })
        } else {
            Ok(left)
        }
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        match self.peek().kind {
            TokenKind::Bang => {
                self.bump();
                let inner = self.parse_unary()?;
                Ok(Expr::Call {
                    target: "#c.not".into(),
                    args: vec![inner],
                })
            }
            TokenKind::Minus => {
                self.bump();
                let inner = self.parse_unary()?;
                Ok(Expr::Call {
                    target: "#c.sub".into(),
                    args: vec![Expr::Num(0.0), inner],
                })
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr> {
        let mut expr = self.parse_factor()?;
        while self.peek().kind == TokenKind::Dot {
            self.bump();
            let ft = self.peek().clone();
            if ft.kind != TokenKind::Ident {
                return Err(Error::at(
                    "parse",
                    ft.line,
                    ft.col,
                    "expected field name after '.'",
                ));
            }
            let field = self.bump().text.clone();
            validate_ident(&field, ft.line, ft.col)?;
            expr = Expr::Field {
                base: Box::new(expr),
                field,
            };
        }
        Ok(expr)
    }

    fn parse_factor(&mut self) -> Result<Expr> {
        let t = self.peek().clone();
        match t.kind {
            TokenKind::Number => Ok(Expr::Num(self.parse_number_lit()?)),
            TokenKind::String => {
                let s = self.bump().text.clone();
                Ok(Expr::Str(s))
            }
            TokenKind::LBracket => self.parse_list_lit(),
            TokenKind::LBrace => self.parse_record_lit(),
            TokenKind::Ident | TokenKind::Address => {
                if t.kind == TokenKind::Ident {
                    let text = self.peek().text.clone();
                    if text == "true" {
                        self.bump();
                        return Ok(Expr::Bool(true));
                    }
                    if text == "false" {
                        self.bump();
                        return Ok(Expr::Bool(false));
                    }
                    if text == "if" || text == "then" || text == "else" || text == "match" {
                        return Err(Error::at(
                            "parse",
                            t.line,
                            t.col,
                            format!("unexpected keyword {text} in expression"),
                        ));
                    }
                }
                let path = self.parse_path_name()?;
                if self.peek().kind == TokenKind::LParen {
                    self.bump();
                    let args = self.parse_args()?;
                    self.expect(TokenKind::RParen)?;
                    Ok(Expr::Call {
                        target: path,
                        args,
                    })
                } else if path.contains("::") {
                    Err(Error::at(
                        "parse",
                        t.line,
                        t.col,
                        "qualified path must be called: mod::name(...)",
                    ))
                } else if path.starts_with('#') {
                    Err(Error::at(
                        "parse",
                        t.line,
                        t.col,
                        "bare address is not an expression; use #addr(...)",
                    ))
                } else {
                    Ok(Expr::Var(path))
                }
            }
            TokenKind::LParen => {
                self.bump();
                let e = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                Ok(e)
            }
            _ => Err(Error::at(
                "parse",
                t.line,
                t.col,
                format!("unexpected token in expression: {:?}", t.kind),
            )),
        }
    }

    fn parse_list_lit(&mut self) -> Result<Expr> {
        self.expect(TokenKind::LBracket)?;
        let mut elems = Vec::new();
        if self.peek().kind != TokenKind::RBracket {
            loop {
                elems.push(self.parse_expr()?);
                if self.peek().kind == TokenKind::Comma {
                    self.bump();
                    continue;
                }
                break;
            }
        }
        self.expect(TokenKind::RBracket)?;
        Ok(Expr::List(elems))
    }

    fn parse_record_lit(&mut self) -> Result<Expr> {
        let t0 = self.peek().clone();
        self.expect(TokenKind::LBrace)?;
        let mut fields = Vec::new();
        let mut seen = BTreeMap::new();
        if self.peek().kind != TokenKind::RBrace {
            loop {
                let name_tok = self.peek().clone();
                if name_tok.kind != TokenKind::Ident {
                    return Err(Error::at(
                        "parse",
                        name_tok.line,
                        name_tok.col,
                        "expected field name in record",
                    ));
                }
                let name = self.bump().text.clone();
                validate_ident(&name, name_tok.line, name_tok.col)?;
                // Field punning: `{ x, y }` means `{ x: x, y: y }`
                let value = if self.peek().kind == TokenKind::Colon {
                    self.bump();
                    self.parse_expr()?
                } else if matches!(self.peek().kind, TokenKind::Comma | TokenKind::RBrace) {
                    Expr::Var(name.clone())
                } else {
                    return Err(Error::at(
                        "parse",
                        name_tok.line,
                        name_tok.col,
                        "expected ':' or field punning in record literal",
                    ));
                };
                if seen.insert(name.clone(), ()).is_some() {
                    return Err(Error::at(
                        "parse",
                        name_tok.line,
                        name_tok.col,
                        format!("duplicate record field {name}"),
                    ));
                }
                fields.push((name, value));
                if self.peek().kind == TokenKind::Comma {
                    self.bump();
                    continue;
                }
                break;
            }
        }
        self.expect(TokenKind::RBrace)?;
        if fields.is_empty() {
            return Err(Error::at(
                "parse",
                t0.line,
                t0.col,
                "record literal needs at least one field",
            ));
        }
        Ok(Expr::Record(fields))
    }

    fn parse_args(&mut self) -> Result<Vec<Expr>> {
        let mut args = Vec::new();
        if self.peek().kind == TokenKind::RParen {
            return Ok(args);
        }
        loop {
            args.push(self.parse_expr()?);
            if self.peek().kind == TokenKind::Comma {
                self.bump();
                continue;
            }
            break;
        }
        Ok(args)
    }

    fn parse_number_lit(&mut self) -> Result<f64> {
        let t = self.peek().clone();
        if t.kind != TokenKind::Number {
            return Err(Error::at("parse", t.line, t.col, "expected number"));
        }
        let text = self.bump().text.clone();
        text.parse::<f64>()
            .map_err(|_| Error::at("parse", t.line, t.col, format!("invalid number {text}")))
    }
}

pub fn validate_ident(name: &str, line: usize, col: usize) -> Result<()> {
    if name.is_empty() || name.len() > 64 {
        return Err(Error::at(
            "lex",
            line,
            col,
            "identifier length must be 1..=64",
        ));
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(Error::at(
            "lex",
            line,
            col,
            format!("identifier must start with letter or _: {name}"),
        ));
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(Error::at(
            "lex",
            line,
            col,
            format!("identifier has illegal characters: {name}"),
        ));
    }
    match name {
        "Num" | "Bool" | "Str" | "List" | "print" | "true" | "false" | "if" | "then" | "else"
        | "type" | "match" | "module" | "import" | "as" => Err(Error::at(
            "lex",
            line,
            col,
            format!("reserved identifier: {name}"),
        )),
        _ => Ok(()),
    }
}
