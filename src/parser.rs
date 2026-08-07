use crate::ast::*;
use crate::error::{Error, Result};
use crate::lexer::{Token, TokenKind};

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

    pub fn parse_program(&mut self) -> Result<Vec<RawBucket>> {
        let mut buckets = Vec::new();
        while self.peek().kind != TokenKind::Eof {
            buckets.push(self.parse_item()?);
        }
        Ok(buckets)
    }

    fn parse_item(&mut self) -> Result<RawBucket> {
        let mut tests = Vec::new();
        let mut is_entry = false;

        loop {
            match self.peek().kind {
                TokenKind::AtTest => {
                    self.bump();
                    tests.push(self.parse_test_ann()?);
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

    fn parse_test_ann(&mut self) -> Result<TestAnn> {
        let (call_target, args) = self.parse_call_head()?;
        self.expect(TokenKind::EqEq)?;
        let expected = self.parse_expr()?;
        Ok(TestAnn {
            call_target,
            args,
            expected,
        })
    }

    fn parse_call_head(&mut self) -> Result<(String, Vec<Expr>)> {
        let target = match self.peek().kind {
            TokenKind::Ident | TokenKind::Address => self.bump().text.clone(),
            _ => {
                let t = self.peek();
                return Err(Error::at(
                    "parse",
                    t.line,
                    t.col,
                    "expected call target in @test",
                ));
            }
        };
        self.expect(TokenKind::LParen)?;
        let args = self.parse_args()?;
        self.expect(TokenKind::RParen)?;
        Ok((target, args))
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

        self.expect(TokenKind::LBrace)?;
        let body = self.parse_block_body()?;
        self.expect(TokenKind::RBrace)?;

        Ok(RawBucket {
            explicit_addr,
            label,
            contract: Contract { params, ret },
            desc,
            body,
            is_entry: false,
            tests: Vec::new(),
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
        let t = self.peek().clone();
        if t.kind != TokenKind::Ident {
            return Err(Error::at("parse", t.line, t.col, "expected type name"));
        }
        let name = self.bump().text.clone();
        match name.as_str() {
            "Num" => Ok(Type::Num),
            "Bool" => Ok(Type::Bool),
            "Str" => Ok(Type::Str),
            other => Err(Error::at(
                "parse",
                t.line,
                t.col,
                format!("unknown type {other} (want Num, Bool, or Str)"),
            )),
        }
    }

    fn parse_expr(&mut self) -> Result<Expr> {
        self.parse_or()
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
        let mut left = self.parse_unary()?;
        loop {
            match self.peek().kind {
                TokenKind::Star => {
                    self.bump();
                    let right = self.parse_unary()?;
                    left = Expr::Call {
                        target: "#c.mul".into(),
                        args: vec![left, right],
                    };
                }
                TokenKind::Slash => {
                    self.bump();
                    let right = self.parse_unary()?;
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
            _ => self.parse_factor(),
        }
    }

    fn parse_factor(&mut self) -> Result<Expr> {
        let t = self.peek().clone();
        match t.kind {
            TokenKind::Number => Ok(Expr::Num(self.parse_number_lit()?)),
            TokenKind::String => {
                let s = self.bump().text.clone();
                Ok(Expr::Str(s))
            }
            TokenKind::Ident | TokenKind::Address => {
                let target = self.bump().text.clone();
                if target == "true" {
                    return Ok(Expr::Bool(true));
                }
                if target == "false" {
                    return Ok(Expr::Bool(false));
                }
                if self.peek().kind == TokenKind::LParen {
                    self.bump();
                    let args = self.parse_args()?;
                    self.expect(TokenKind::RParen)?;
                    Ok(Expr::Call { target, args })
                } else if t.kind == TokenKind::Ident {
                    Ok(Expr::Var(target))
                } else {
                    Err(Error::at(
                        "parse",
                        t.line,
                        t.col,
                        "bare address is not an expression; use #addr(...)",
                    ))
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
        "Num" | "Bool" | "Str" | "print" | "true" | "false" => Err(Error::at(
            "lex",
            line,
            col,
            format!("reserved identifier: {name}"),
        )),
        _ => Ok(()),
    }
}
