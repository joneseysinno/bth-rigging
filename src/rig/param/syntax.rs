//! Expression syntax for rig parameters.
//!
//! Step: 2.5
//! Theory: docs/step-2.5-parser-param-table.md, §§2–5.
//! Inputs: parameter expression text.
//! Outputs: checked parameter expressions with source spans and diagnostics.
//! Must not depend on: UI, dioxus, store, solver internals.

use std::collections::HashMap;
use std::str::FromStr;

use super::{Expr, ParamId, ParamTable, Quantity, RigErrorKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Unit {
    Feet,
    Inches,
    Pounds,
    Kips,
    Degrees,
    Ratio,
    Ton,
    KipsPlural,
    Millimeters,
    Meters,
    Kilograms,
}

#[derive(Debug, Clone, PartialEq)]
enum TokenKind {
    Number {
        value: f64,
        unit: Option<Unit>,
        unit_span: Option<Span>,
    },
    FeetInches {
        feet: f64,
        inches: f64,
    },
    Ident(String),
    Plus,
    Minus,
    Star,
    DoubleStar,
    Slash,
    Caret,
    LParen,
    RParen,
    Apostrophe,
    Quote,
    Degree,
    Percent,
    End,
}

#[derive(Debug, Clone, PartialEq)]
struct Token {
    kind: TokenKind,
    span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LexError {
    kind: ParseErrorKind,
    span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseErrorKind {
    Empty,
    UnexpectedChar,
    UnexpectedToken,
    UnexpectedEnd,
    UnclosedParen,
    BadNumber,
    UnknownName,
    UnknownUnit,
    Unsupported,
    QuantityMismatch,
    WrongQuantity,
    NonFinite,
    DivideByZero,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub span: Span,
    pub column: usize,
    pub message: String,
    pub help: Option<String>,
}

impl ParseError {
    pub fn caret(&self, src: &str) -> String {
        let start = self.span.start.min(src.len());
        let end = self.span.end.min(src.len()).max(start);
        let prefix_chars = src[..start].chars().count();
        let span_chars = src[start..end].chars().count().max(1);
        format!(
            "{src}\n{}{}",
            " ".repeat(prefix_chars),
            "^".repeat(span_chars)
        )
    }
}

pub fn column(src: &str, byte_offset: usize) -> usize {
    src[..byte_offset.min(src.len())].chars().count() + 1
}

fn lex(src: &str) -> Result<Vec<Token>, LexError> {
    let bytes = src.as_bytes();
    let mut offset = 0;
    let mut tokens = Vec::new();

    while offset < bytes.len() {
        let ch = src[offset..]
            .chars()
            .next()
            .expect("offset is a char boundary");
        if ch.is_whitespace() {
            offset += ch.len_utf8();
            continue;
        }

        let start = offset;
        let kind = match bytes[offset] {
            b'0'..=b'9' => scan_number(src, &mut offset)?,
            b'.' if bytes.get(offset + 1).is_some_and(u8::is_ascii_digit) => {
                scan_number(src, &mut offset)?
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                let end = scan_ident(bytes, offset);
                let name = src[offset..end].to_string();
                offset = end;
                TokenKind::Ident(name)
            }
            b'+' => {
                offset += 1;
                TokenKind::Plus
            }
            b'-' => {
                offset += 1;
                TokenKind::Minus
            }
            b'*' if bytes.get(offset + 1) == Some(&b'*') => {
                offset += 2;
                TokenKind::DoubleStar
            }
            b'*' => {
                offset += 1;
                TokenKind::Star
            }
            b'/' => {
                offset += 1;
                TokenKind::Slash
            }
            b'^' => {
                offset += 1;
                TokenKind::Caret
            }
            b'(' => {
                offset += 1;
                TokenKind::LParen
            }
            b')' => {
                offset += 1;
                TokenKind::RParen
            }
            b'\'' => {
                offset += 1;
                TokenKind::Apostrophe
            }
            b'"' => {
                offset += 1;
                TokenKind::Quote
            }
            b'%' => {
                offset += 1;
                TokenKind::Percent
            }
            _ if ch == '°' => {
                offset += ch.len_utf8();
                TokenKind::Degree
            }
            _ => {
                return Err(LexError {
                    kind: ParseErrorKind::UnexpectedChar,
                    span: Span {
                        start,
                        end: start + ch.len_utf8(),
                    },
                });
            }
        };
        tokens.push(Token {
            kind,
            span: Span { start, end: offset },
        });
    }

    tokens.push(Token {
        kind: TokenKind::End,
        span: Span {
            start: src.len(),
            end: src.len(),
        },
    });
    Ok(tokens)
}

fn scan_ident(bytes: &[u8], start: usize) -> usize {
    if !bytes
        .get(start)
        .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
    {
        return start;
    }
    let mut end = start + 1;
    while bytes
        .get(end)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        end += 1;
    }
    end
}

fn scan_number(src: &str, offset: &mut usize) -> Result<TokenKind, LexError> {
    let bytes = src.as_bytes();
    let start = *offset;
    let mut end = start;

    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if bytes.get(end) == Some(&b'.') {
        end += 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let exponent = end;
        end += 1;
        if matches!(bytes.get(end), Some(b'+' | b'-')) {
            end += 1;
        }
        let digits = end;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        if end == digits {
            return Err(LexError {
                kind: ParseErrorKind::BadNumber,
                span: Span {
                    start: exponent,
                    end,
                },
            });
        }
    }

    let number_end = end;
    let value = f64::from_str(&src[start..number_end]).map_err(|_| LexError {
        kind: ParseErrorKind::BadNumber,
        span: Span {
            start,
            end: number_end,
        },
    })?;

    if let Some((feet, inches, compound_end)) = scan_feet_inches(src, number_end, value) {
        *offset = compound_end;
        return Ok(TokenKind::FeetInches { feet, inches });
    }

    let (unit, unit_span, unit_end) = scan_unit_suffix(src, number_end);
    *offset = unit_end;
    Ok(TokenKind::Number {
        value,
        unit,
        unit_span,
    })
}

fn scan_feet_inches(src: &str, start: usize, feet: f64) -> Option<(f64, f64, usize)> {
    let bytes = src.as_bytes();
    if bytes.get(start) != Some(&b'\'') {
        return None;
    }
    let mut offset = start + 1;
    if bytes.get(offset) == Some(&b'-') {
        offset += 1;
    }
    let inches_start = offset;
    while bytes.get(offset).is_some_and(u8::is_ascii_digit) {
        offset += 1;
    }
    if bytes.get(offset) == Some(&b'.') {
        offset += 1;
        while bytes.get(offset).is_some_and(u8::is_ascii_digit) {
            offset += 1;
        }
    }
    if offset == inches_start || bytes.get(offset) != Some(&b'"') {
        return None;
    }
    let inches = f64::from_str(&src[inches_start..offset]).ok()?;
    Some((feet, inches, offset + 1))
}

fn scan_unit_suffix(src: &str, number_end: usize) -> (Option<Unit>, Option<Span>, usize) {
    let bytes = src.as_bytes();
    let mut start = number_end;
    while let Some(ch) = src[start..].chars().next()
        && ch.is_whitespace()
    {
        start += ch.len_utf8();
    }

    let symbol = match bytes.get(start) {
        Some(b'\'') => Some(Unit::Feet),
        Some(b'"') => Some(Unit::Inches),
        Some(b'%') => Some(Unit::Ratio),
        _ if src[start..].starts_with('°') => Some(Unit::Degrees),
        _ => None,
    };
    if let Some(unit) = symbol {
        let width = if src[start..].starts_with('°') {
            '°'.len_utf8()
        } else {
            1
        };
        let end = start + width;
        return (Some(unit), Some(Span { start, end }), end);
    }

    let word_end = scan_ident(bytes, start);
    if word_end == start {
        return (None, None, number_end);
    }
    let unit = match &src[start..word_end] {
        "ft" => Some(Unit::Feet),
        "in" => Some(Unit::Inches),
        "lb" | "lbs" => Some(Unit::Pounds),
        "kip" => Some(Unit::Kips),
        "deg" => Some(Unit::Degrees),
        "ton" => Some(Unit::Ton),
        "kips" => Some(Unit::KipsPlural),
        "mm" => Some(Unit::Millimeters),
        "m" => Some(Unit::Meters),
        "kg" => Some(Unit::Kilograms),
        _ => None,
    };
    unit.map(|unit| {
        (
            Some(unit),
            Some(Span {
                start,
                end: word_end,
            }),
            word_end,
        )
    })
    .unwrap_or((None, None, number_end))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone, PartialEq)]
enum SyntaxKind {
    Literal {
        value: f64,
        unit: Option<Unit>,
        unit_span: Option<Span>,
    },
    Ident(String),
    Neg(Box<SpannedExpr>),
    Binary {
        op: BinaryOp,
        op_span: Span,
        left: Box<SpannedExpr>,
        right: Box<SpannedExpr>,
    },
    Group(Box<SpannedExpr>),
}

#[derive(Debug, Clone, PartialEq)]
struct SpannedExpr {
    kind: SyntaxKind,
    span: Span,
}

fn parse_syntax(src: &str) -> Result<SpannedExpr, ParseError> {
    let tokens = lex(src).map_err(|error| ParseError {
        kind: error.kind,
        span: error.span,
        column: column(src, error.span.start),
        message: match error.kind {
            ParseErrorKind::UnexpectedChar => "unexpected character".into(),
            ParseErrorKind::BadNumber => "invalid number".into(),
            _ => unreachable!(),
        },
        help: None,
    })?;
    let mut parser = Parser {
        src,
        tokens,
        cursor: 0,
    };
    if matches!(parser.peek().kind, TokenKind::End) {
        return Err(parser.error(
            ParseErrorKind::Empty,
            Span { start: 0, end: 0 },
            "expression is empty",
            None,
        ));
    }
    let expression = parser.parse_binary(0)?;
    if !matches!(parser.peek().kind, TokenKind::End) {
        return Err(parser.trailing_error());
    }
    Ok(expression)
}

struct Parser<'a> {
    src: &'a str,
    tokens: Vec<Token>,
    cursor: usize,
}

impl Parser<'_> {
    fn peek(&self) -> &Token {
        &self.tokens[self.cursor]
    }

    fn take(&mut self) -> Token {
        let token = self.tokens[self.cursor].clone();
        self.cursor += 1;
        token
    }

    fn error(
        &self,
        kind: ParseErrorKind,
        span: Span,
        message: impl Into<String>,
        help: Option<&str>,
    ) -> ParseError {
        ParseError {
            kind,
            span,
            column: column(self.src, span.start),
            message: message.into(),
            help: help.map(str::to_owned),
        }
    }

    fn parse_binary(&mut self, min_precedence: u8) -> Result<SpannedExpr, ParseError> {
        let mut left = self.parse_unary()?;
        loop {
            let token = self.peek().clone();
            let (op, precedence) = match token.kind {
                TokenKind::Plus => (BinaryOp::Add, 1),
                TokenKind::Minus => (BinaryOp::Sub, 1),
                TokenKind::Star => (BinaryOp::Mul, 2),
                TokenKind::Slash => (BinaryOp::Div, 2),
                TokenKind::Caret | TokenKind::DoubleStar | TokenKind::Percent => {
                    return Err(self.error(
                        ParseErrorKind::Unsupported,
                        token.span,
                        "operator isn't supported",
                        Some("functions aren't supported yet"),
                    ));
                }
                _ => {
                    if starts_atom(&token.kind) {
                        return Err(self.error(
                            ParseErrorKind::Unsupported,
                            token.span,
                            "implicit multiplication isn't supported",
                            None,
                        ));
                    }
                    break;
                }
            };
            if precedence < min_precedence {
                break;
            }
            self.take();
            let right = self.parse_binary(precedence + 1)?;
            let span = Span {
                start: left.span.start,
                end: right.span.end,
            };
            left = SpannedExpr {
                kind: SyntaxKind::Binary {
                    op,
                    op_span: token.span,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<SpannedExpr, ParseError> {
        if matches!(self.peek().kind, TokenKind::Minus) {
            let minus = self.take();
            let operand = self.parse_unary()?;
            if let SyntaxKind::Literal {
                value,
                unit,
                unit_span,
            } = &operand.kind
            {
                return Ok(SpannedExpr {
                    kind: SyntaxKind::Literal {
                        value: -*value,
                        unit: unit.clone(),
                        unit_span: *unit_span,
                    },
                    span: Span {
                        start: minus.span.start,
                        end: operand.span.end,
                    },
                });
            }
            return Ok(SpannedExpr {
                span: Span {
                    start: minus.span.start,
                    end: operand.span.end,
                },
                kind: SyntaxKind::Neg(Box::new(operand)),
            });
        }
        self.parse_atom()
    }

    fn parse_atom(&mut self) -> Result<SpannedExpr, ParseError> {
        let token = self.peek().clone();
        match token.kind {
            TokenKind::Number {
                value,
                unit,
                unit_span,
            } => {
                self.take();
                Ok(SpannedExpr {
                    kind: SyntaxKind::Literal {
                        value,
                        unit,
                        unit_span,
                    },
                    span: token.span,
                })
            }
            TokenKind::FeetInches { feet, inches } => {
                self.take();
                Ok(SpannedExpr {
                    kind: SyntaxKind::Literal {
                        value: feet + inches / 12.0,
                        unit: Some(Unit::Feet),
                        unit_span: None,
                    },
                    span: token.span,
                })
            }
            TokenKind::Ident(name) => {
                self.take();
                if matches!(self.peek().kind, TokenKind::LParen) {
                    return Err(self.error(
                        ParseErrorKind::Unsupported,
                        token.span,
                        "function calls aren't supported",
                        Some("functions aren't supported yet"),
                    ));
                }
                Ok(SpannedExpr {
                    kind: SyntaxKind::Ident(name),
                    span: token.span,
                })
            }
            TokenKind::LParen => {
                let open = self.take();
                let inner = self.parse_binary(0)?;
                if matches!(self.peek().kind, TokenKind::End) {
                    return Err(self.error(
                        ParseErrorKind::UnclosedParen,
                        open.span,
                        "unclosed parenthesis",
                        None,
                    ));
                }
                if !matches!(self.peek().kind, TokenKind::RParen) {
                    let token = self.peek().clone();
                    return Err(self.error(
                        ParseErrorKind::UnexpectedToken,
                        token.span,
                        "expected ')'",
                        None,
                    ));
                }
                let close = self.take();
                Ok(SpannedExpr {
                    kind: SyntaxKind::Group(Box::new(inner)),
                    span: Span {
                        start: open.span.start,
                        end: close.span.end,
                    },
                })
            }
            TokenKind::End => Err(self.error(
                ParseErrorKind::UnexpectedEnd,
                token.span,
                "unexpected end of expression",
                None,
            )),
            _ => Err(self.error(
                ParseErrorKind::UnexpectedToken,
                token.span,
                "expected a number, name, or parenthesized expression",
                None,
            )),
        }
    }

    fn trailing_error(&self) -> ParseError {
        let token = self.peek();
        self.error(
            ParseErrorKind::UnexpectedToken,
            token.span,
            "unexpected token after expression",
            None,
        )
    }
}

fn starts_atom(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Number { .. }
            | TokenKind::FeetInches { .. }
            | TokenKind::Ident(_)
            | TokenKind::LParen
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    pub expr: Expr,
    pub quantity: Option<Quantity>,
    pub is_constant: bool,
    pub lints: Vec<Lint>,
    span: Span,
    bare_length_suspicion: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lint {
    BareLengthLooksLikeInches { span: Span },
}

pub fn parse_expr(src: &str, table: &ParamTable) -> Result<Parsed, ParseError> {
    let syntax = parse_syntax(src)?;
    let names: HashMap<&str, (ParamId, Quantity)> = table
        .values()
        .map(|param| (param.name.as_str(), (param.id, param.quantity)))
        .collect();
    let (expr, quantity) = check_and_lower(&syntax, src, table, &names)?;
    let bare_length_suspicion = matches!(
        syntax.kind,
        SyntaxKind::Literal {
            value,
            unit: None,
            ..
        } if value > 60.0
    );
    Ok(Parsed {
        is_constant: expr.params().is_empty(),
        expr,
        quantity,
        lints: Vec::new(),
        span: syntax.span,
        bare_length_suspicion,
    })
}

pub fn parse_for(src: &str, table: &ParamTable, expected: Quantity) -> Result<Parsed, ParseError> {
    let mut parsed = parse_expr(src, table)?;
    if let Some(actual) = parsed.quantity
        && actual != expected
    {
        return Err(ParseError {
            kind: ParseErrorKind::WrongQuantity,
            span: parsed.span,
            column: column(src, parsed.span.start),
            message: format!("expected {expected:?}, got {actual:?}"),
            help: None,
        });
    }
    if expected == Quantity::Length && parsed.bare_length_suspicion {
        parsed
            .lints
            .push(Lint::BareLengthLooksLikeInches { span: parsed.span });
    }
    Ok(parsed)
}

fn check_and_lower(
    node: &SpannedExpr,
    src: &str,
    table: &ParamTable,
    names: &HashMap<&str, (ParamId, Quantity)>,
) -> Result<(Expr, Option<Quantity>), ParseError> {
    match &node.kind {
        SyntaxKind::Literal {
            value,
            unit,
            unit_span,
        } => {
            if !value.is_finite() {
                return Err(parse_error(
                    src,
                    ParseErrorKind::NonFinite,
                    node.span,
                    "number must be finite",
                    None,
                ));
            }
            match unit {
                None => Ok((Expr::Const(*value), None)),
                Some(unit) => {
                    let (quantity, factor) = unit_conversion(unit).ok_or_else(|| {
                        let label = unit_name(unit);
                        let help = (unit == &Unit::Ton).then_some("short or metric? use kip or lb");
                        parse_error(
                            src,
                            ParseErrorKind::UnknownUnit,
                            unit_span.unwrap_or(node.span),
                            format!("unknown unit '{label}'"),
                            help,
                        )
                    })?;
                    let base_value = value * factor;
                    if !base_value.is_finite() {
                        return Err(parse_error(
                            src,
                            ParseErrorKind::NonFinite,
                            node.span,
                            "converted number must be finite",
                            None,
                        ));
                    }
                    Ok((Expr::Const(base_value), Some(quantity)))
                }
            }
        }
        SyntaxKind::Ident(name) => {
            if let Some(&(id, quantity)) = names.get(name.as_str()) {
                return Ok((Expr::Param(id), Some(quantity)));
            }
            let suggestion = suggest_name(name, table);
            let help = suggestion.map(|name| format!("did you mean `{name}`?"));
            Err(parse_error(
                src,
                ParseErrorKind::UnknownName,
                node.span,
                format!("unknown parameter '{name}'"),
                help.as_deref(),
            ))
        }
        SyntaxKind::Neg(child) => {
            let (expr, quantity) = check_and_lower(child, src, table, names)?;
            Ok((Expr::Neg(Box::new(expr)), quantity))
        }
        SyntaxKind::Group(child) => check_and_lower(child, src, table, names),
        SyntaxKind::Binary {
            op,
            op_span,
            left,
            right,
        } => {
            let (left_expr, left_quantity) = check_and_lower(left, src, table, names)?;
            let (right_expr, right_quantity) = check_and_lower(right, src, table, names)?;
            if *op == BinaryOp::Div && is_literal_zero(right) {
                return Err(parse_error(
                    src,
                    ParseErrorKind::DivideByZero,
                    *op_span,
                    "division by zero",
                    None,
                ));
            }
            let merged = match op {
                BinaryOp::Add | BinaryOp::Sub => super::merge_add(left_quantity, right_quantity),
                BinaryOp::Mul => super::merge_mul(left_quantity, right_quantity),
                BinaryOp::Div => super::merge_div(left_quantity, right_quantity),
            }
            .map_err(|error| {
                let kind = match error.code {
                    RigErrorKind::QuantityMismatch => ParseErrorKind::QuantityMismatch,
                    _ => ParseErrorKind::UnexpectedToken,
                };
                parse_error(src, kind, *op_span, error.message, None)
            })?;
            let expr = match op {
                BinaryOp::Add => Expr::Add(Box::new(left_expr), Box::new(right_expr)),
                BinaryOp::Sub => Expr::Sub(Box::new(left_expr), Box::new(right_expr)),
                BinaryOp::Mul => Expr::Mul(Box::new(left_expr), Box::new(right_expr)),
                BinaryOp::Div => Expr::Div(Box::new(left_expr), Box::new(right_expr)),
            };
            Ok((expr, merged))
        }
    }
}

fn is_literal_zero(node: &SpannedExpr) -> bool {
    matches!(node.kind, SyntaxKind::Literal { value: 0.0, .. })
}

fn unit_conversion(unit: &Unit) -> Option<(Quantity, f64)> {
    match unit {
        Unit::Feet => Some((Quantity::Length, 1.0)),
        Unit::Inches => Some((Quantity::Length, 1.0 / 12.0)),
        Unit::Pounds => Some((Quantity::Weight, 1.0)),
        Unit::Kips => Some((Quantity::Weight, 1000.0)),
        Unit::Degrees => Some((Quantity::Angle, 1.0)),
        Unit::Ratio => Some((Quantity::Ratio, 0.01)),
        Unit::Ton | Unit::KipsPlural | Unit::Millimeters | Unit::Meters | Unit::Kilograms => None,
    }
}

fn unit_name(unit: &Unit) -> &'static str {
    match unit {
        Unit::Feet => "ft",
        Unit::Inches => "in",
        Unit::Pounds => "lb",
        Unit::Kips => "kip",
        Unit::Degrees => "deg",
        Unit::Ratio => "%",
        Unit::Ton => "ton",
        Unit::KipsPlural => "kips",
        Unit::Millimeters => "mm",
        Unit::Meters => "m",
        Unit::Kilograms => "kg",
    }
}

fn parse_error(
    src: &str,
    kind: ParseErrorKind,
    span: Span,
    message: impl Into<String>,
    help: Option<&str>,
) -> ParseError {
    ParseError {
        kind,
        span,
        column: column(src, span.start),
        message: message.into(),
        help: help.map(str::to_owned),
    }
}

fn suggest_name<'a>(name: &str, table: &'a ParamTable) -> Option<&'a str> {
    if let Some(param) = table
        .values()
        .find(|param| param.name.eq_ignore_ascii_case(name))
    {
        return Some(&param.name);
    }
    table
        .values()
        .filter_map(|param| {
            bounded_levenshtein(name, &param.name).map(|distance| (distance, param.name.as_str()))
        })
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, candidate)| candidate)
}

fn bounded_levenshtein(left: &str, right: &str) -> Option<usize> {
    let left = left.to_ascii_lowercase();
    let right = right.to_ascii_lowercase();
    let left = left.as_bytes();
    let right = right.as_bytes();
    if left.len().abs_diff(right.len()) > 2 {
        return None;
    }
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0; right.len() + 1];
    for (i, left_byte) in left.iter().enumerate() {
        current[0] = i + 1;
        let mut row_min = current[0];
        for (j, right_byte) in right.iter().enumerate() {
            current[j + 1] = (previous[j + 1] + 1)
                .min(current[j] + 1)
                .min(previous[j] + usize::from(left_byte != right_byte));
            row_min = row_min.min(current[j + 1]);
        }
        if row_min > 2 {
            return None;
        }
        std::mem::swap(&mut previous, &mut current);
    }
    (previous[right.len()] <= 2).then_some(previous[right.len()])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        lex(src)
            .unwrap()
            .into_iter()
            .map(|token| token.kind)
            .collect()
    }

    #[test]
    fn lexes_each_token_kind_and_unit_suffix() {
        let tokens =
            lex("n + 1 - 2*3 / 4 ^ (5) ** 6 7' 8\" 9° 10% 11 ft 12 lb 13 lbs 14 kip 15 deg")
                .unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Ident(_)));
        assert!(matches!(tokens[1].kind, TokenKind::Plus));
        assert!(matches!(tokens[2].kind, TokenKind::Number { .. }));
        assert!(matches!(tokens[3].kind, TokenKind::Minus));
        assert!(matches!(tokens[4].kind, TokenKind::Number { .. }));
        assert!(matches!(tokens[5].kind, TokenKind::Star));
        assert!(matches!(tokens[6].kind, TokenKind::Number { .. }));
        assert!(matches!(tokens[7].kind, TokenKind::Slash));
        assert!(matches!(tokens[8].kind, TokenKind::Number { .. }));
        assert!(matches!(tokens[9].kind, TokenKind::Caret));
        assert!(matches!(tokens[10].kind, TokenKind::LParen));
        assert!(matches!(tokens[11].kind, TokenKind::Number { .. }));
        assert!(matches!(tokens[12].kind, TokenKind::RParen));
        assert!(matches!(tokens[13].kind, TokenKind::DoubleStar));
        assert!(matches!(tokens[14].kind, TokenKind::Number { .. }));
        assert!(matches!(
            tokens[15].kind,
            TokenKind::Number {
                unit: Some(Unit::Feet),
                ..
            }
        ));
        assert!(matches!(
            tokens[16].kind,
            TokenKind::Number {
                unit: Some(Unit::Inches),
                ..
            }
        ));
        assert!(matches!(
            tokens[17].kind,
            TokenKind::Number {
                unit: Some(Unit::Degrees),
                ..
            }
        ));
        assert!(matches!(
            tokens[18].kind,
            TokenKind::Number {
                unit: Some(Unit::Ratio),
                ..
            }
        ));
        assert!(matches!(
            tokens[19].kind,
            TokenKind::Number {
                unit: Some(Unit::Feet),
                ..
            }
        ));
        assert!(matches!(
            tokens[20].kind,
            TokenKind::Number {
                unit: Some(Unit::Pounds),
                ..
            }
        ));
        assert!(matches!(
            tokens[21].kind,
            TokenKind::Number {
                unit: Some(Unit::Pounds),
                ..
            }
        ));
        assert!(matches!(
            tokens[22].kind,
            TokenKind::Number {
                unit: Some(Unit::Kips),
                ..
            }
        ));
        assert!(matches!(
            tokens[23].kind,
            TokenKind::Number {
                unit: Some(Unit::Degrees),
                ..
            }
        ));
        assert!(matches!(tokens[24].kind, TokenKind::End));

        assert!(matches!(kinds("'")[0], TokenKind::Apostrophe));
        assert!(matches!(kinds("\"")[0], TokenKind::Quote));
        assert!(matches!(kinds("°")[0], TokenKind::Degree));
        assert!(matches!(kinds("%")[0], TokenKind::Percent));
    }

    #[test]
    fn feet_inches_requires_no_internal_whitespace() {
        let compact = lex("8'-6\"").unwrap();
        assert_eq!(compact.len() - 1, 1);
        assert!(matches!(
            compact[0].kind,
            TokenKind::FeetInches {
                feet: 8.0,
                inches: 6.0
            }
        ));

        let spaced = lex("8' - 6\"").unwrap();
        assert_eq!(spaced.len() - 1, 3);
        assert!(matches!(
            spaced[0].kind,
            TokenKind::Number {
                value: 8.0,
                unit: Some(Unit::Feet),
                ..
            }
        ));
        assert!(matches!(spaced[1].kind, TokenKind::Minus));
        assert!(matches!(
            spaced[2].kind,
            TokenKind::Number {
                value: 6.0,
                unit: Some(Unit::Inches),
                ..
            }
        ));
    }

    #[test]
    fn numbers_and_exponents_are_lexed_without_stealing_identifiers() {
        assert!(
            matches!(kinds("1e-3")[0], TokenKind::Number { value, unit: None, .. } if (value - 0.001).abs() < 1e-15)
        );
        assert!(matches!(
            kinds(".5")[0],
            TokenKind::Number { value: 0.5, .. }
        ));
        assert!(matches!(
            kinds("5.")[0],
            TokenKind::Number { value: 5.0, .. }
        ));
        assert!(matches!(kinds("e")[0], TokenKind::Ident(ref name) if name == "e"));
        assert!(matches!(
            kinds("2e3")[0],
            TokenKind::Number { value: 2000.0, .. }
        ));
        assert!(matches!(kinds("2 e")[1], TokenKind::Ident(ref name) if name == "e"));
    }

    #[test]
    fn columns_count_characters_while_spans_count_bytes() {
        let src = "45° + @";
        let degree = lex("45°").unwrap()[0].span;
        assert_eq!(degree, Span { start: 0, end: 4 });
        let err = lex(src).unwrap_err();
        assert_eq!(err.span, Span { start: 7, end: 8 });
        assert_eq!(column(src, err.span.start), 7);
        assert_eq!(column("°x", 2), 2);
    }

    #[test]
    fn parser_obeys_precedence_and_left_associativity() {
        let expression = parse_syntax("a + b*c").unwrap();
        assert!(matches!(
            expression.kind,
            SyntaxKind::Binary {
                op: BinaryOp::Add,
                right,
                ..
            } if matches!(right.kind, SyntaxKind::Binary { op: BinaryOp::Mul, .. })
        ));

        let expression = parse_syntax("a - b - c").unwrap();
        assert!(matches!(
            expression.kind,
            SyntaxKind::Binary {
                op: BinaryOp::Sub,
                left,
                ..
            } if matches!(left.kind, SyntaxKind::Binary { op: BinaryOp::Sub, .. })
        ));

        let expression = parse_syntax("-a*b").unwrap();
        assert!(matches!(
            expression.kind,
            SyntaxKind::Binary {
                op: BinaryOp::Mul,
                left,
                ..
            } if matches!(left.kind, SyntaxKind::Neg(_))
        ));
    }

    #[test]
    fn parser_folds_only_unparenthesized_negative_literals() {
        let folded = parse_syntax("- 2").unwrap();
        assert!(matches!(
            folded.kind,
            SyntaxKind::Literal {
                value: -2.0,
                unit: None,
                ..
            }
        ));

        let grouped = parse_syntax("-(2)").unwrap();
        assert!(matches!(
            grouped.kind,
            SyntaxKind::Neg(inner) if matches!(inner.kind, SyntaxKind::Group(_))
        ));

        let feet_inches = parse_syntax("8'-6\"").unwrap();
        assert!(matches!(
            feet_inches.kind,
            SyntaxKind::Literal {
                value: 8.5,
                unit: Some(Unit::Feet),
                ..
            }
        ));

        assert!(matches!(
            parse_syntax("8' - 6\"").unwrap().kind,
            SyntaxKind::Binary {
                op: BinaryOp::Sub,
                ..
            }
        ));
    }

    #[test]
    fn each_structural_error_reports_its_character_column() {
        let cases = [
            ("", ParseErrorKind::Empty, 1),
            ("@", ParseErrorKind::UnexpectedChar, 1),
            (")", ParseErrorKind::UnexpectedToken, 1),
            ("a +", ParseErrorKind::UnexpectedEnd, 4),
            ("(a", ParseErrorKind::UnclosedParen, 1),
            ("1e+", ParseErrorKind::BadNumber, 2),
            ("a + °", ParseErrorKind::UnexpectedToken, 5),
        ];
        for (source, kind, expected_column) in cases {
            let error = parse_syntax(source).unwrap_err();
            assert_eq!(error.kind, kind, "source: {source:?}");
            assert_eq!(error.column, expected_column, "source: {source:?}");
        }
    }

    #[test]
    fn reserved_and_implicit_forms_are_unsupported() {
        for source in ["sqrt(a)", "a^2", "a**2", "a % b", "2G", "2(a)"] {
            let error = parse_syntax(source).unwrap_err();
            assert_eq!(error.kind, ParseErrorKind::Unsupported, "source: {source}");
        }
        let error = parse_syntax("name(x)").unwrap_err();
        assert_eq!(error.kind, ParseErrorKind::Unsupported);
        assert_eq!(
            error.help.as_deref(),
            Some("functions aren't supported yet")
        );
    }

    #[test]
    fn checked_duplo_expression_matches_builder_structure() {
        let rig = crate::rig::duplo10();
        let s12 = rig.param_named("s12").unwrap();
        let gauge = rig.param_named("lug_gauge_G").unwrap();
        let expected = Expr::Param(s12.id) + Expr::Const(2.0) * Expr::Param(gauge.id);
        let parsed = parse_expr("s12 + 2*lug_gauge_G", &rig.params).unwrap();
        assert_eq!(parsed.expr, expected);
        assert_eq!(parsed.quantity, Some(Quantity::Length));
        assert!(!parsed.is_constant);
    }

    #[test]
    fn unit_literals_convert_to_base_units() {
        let table = ParamTable::new();
        for (source, value, quantity) in [
            ("8'-6\"", 8.5, Quantity::Length),
            ("6 in", 0.5, Quantity::Length),
            ("12 kip", 12_000.0, Quantity::Weight),
            ("45°", 45.0, Quantity::Angle),
            ("50%", 0.5, Quantity::Ratio),
        ] {
            let parsed = parse_expr(source, &table).unwrap();
            assert_eq!(parsed.expr, Expr::Const(value), "source: {source}");
            assert_eq!(parsed.quantity, Some(quantity), "source: {source}");
        }
    }

    #[test]
    fn quantity_errors_point_to_operator_and_expected_type() {
        let mut table = ParamTable::new();
        let length = super::super::Param::new("span_A", Quantity::Length, 10.0);
        let weight = super::super::Param::new("load_weight", Quantity::Weight, 100.0);
        table.insert(length.id, length);
        table.insert(weight.id, weight);

        let error = parse_expr("span_A + load_weight", &table).unwrap_err();
        assert_eq!(error.kind, ParseErrorKind::QuantityMismatch);
        assert_eq!(error.column, 8);
        assert_eq!(
            &"span_A + load_weight"[error.span.start..error.span.end],
            "+"
        );

        let rig = crate::rig::duplo10();
        assert!(parse_for("span_A/s12", &rig.params, Quantity::Ratio).is_ok());
        assert_eq!(
            parse_for("span_A/s12", &rig.params, Quantity::Length)
                .unwrap_err()
                .kind,
            ParseErrorKind::WrongQuantity
        );
    }

    #[test]
    fn unknown_names_units_and_bare_length_lint_are_reported() {
        let rig = crate::rig::duplo10();
        let error = parse_expr("S12", &rig.params).unwrap_err();
        assert_eq!(error.kind, ParseErrorKind::UnknownName);
        assert_eq!(error.help.as_deref(), Some("did you mean `s12`?"));

        let error = parse_expr("2 ton", &rig.params).unwrap_err();
        assert_eq!(error.kind, ParseErrorKind::UnknownUnit);
        assert_eq!(error.column, 3);
        assert_eq!(
            error.help.as_deref(),
            Some("short or metric? use kip or lb")
        );

        for source in ["2 kips", "2 mm", "2 m", "2 kg"] {
            assert_eq!(
                parse_expr(source, &rig.params).unwrap_err().kind,
                ParseErrorKind::UnknownUnit,
                "source: {source}"
            );
        }

        let parsed = parse_for("61", &rig.params, Quantity::Length).unwrap();
        assert_eq!(
            parsed.lints,
            vec![Lint::BareLengthLooksLikeInches {
                span: Span { start: 0, end: 2 }
            }]
        );
        assert!(
            parse_for("61", &rig.params, Quantity::Weight)
                .unwrap()
                .lints
                .is_empty()
        );
    }

    #[test]
    fn constant_division_by_zero_and_non_finite_are_rejected() {
        let table = ParamTable::new();
        let zero = parse_expr("1/0", &table).unwrap_err();
        assert_eq!(zero.kind, ParseErrorKind::DivideByZero);
        assert_eq!(zero.column, 2);
        let non_finite = parse_expr("1e999", &table).unwrap_err();
        assert_eq!(non_finite.kind, ParseErrorKind::NonFinite);
    }
}
