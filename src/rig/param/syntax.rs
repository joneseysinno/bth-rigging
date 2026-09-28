//! Expression syntax for rig parameters.
//!
//! Step: 2.5
//! Theory: docs/step-2.5-parser-param-table.md, §§2–5.
//! Inputs: parameter expression text.
//! Outputs: byte-spanned tokens and, in later work items, checked `Expr` values.
//! Must not depend on: UI, dioxus, store, solver internals.

#![allow(dead_code)]

use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    Feet,
    Inches,
    Pounds,
    Kips,
    Degrees,
    Ratio,
}

#[derive(Debug, Clone, PartialEq)]
enum TokenKind {
    Number { value: f64, unit: Option<Unit> },
    FeetInches { feet: f64, inches: f64 },
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
    span: Span,
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
                span: Span {
                    start: exponent,
                    end,
                },
            });
        }
    }

    let number_end = end;
    let value = f64::from_str(&src[start..number_end]).map_err(|_| LexError {
        span: Span {
            start,
            end: number_end,
        },
    })?;

    if let Some((feet, inches, compound_end)) = scan_feet_inches(src, number_end, value) {
        *offset = compound_end;
        return Ok(TokenKind::FeetInches { feet, inches });
    }

    let (unit, unit_end) = scan_unit_suffix(src, number_end);
    *offset = unit_end;
    Ok(TokenKind::Number { value, unit })
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

fn scan_unit_suffix(src: &str, number_end: usize) -> (Option<Unit>, usize) {
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
        return (Some(unit), start + width);
    }

    let word_end = scan_ident(bytes, start);
    if word_end == start {
        return (None, number_end);
    }
    let unit = match &src[start..word_end] {
        "ft" => Some(Unit::Feet),
        "in" => Some(Unit::Inches),
        "lb" | "lbs" => Some(Unit::Pounds),
        "kip" => Some(Unit::Kips),
        "deg" => Some(Unit::Degrees),
        _ => None,
    };
    unit.map(|unit| (Some(unit), word_end))
        .unwrap_or((None, number_end))
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
                unit: Some(Unit::Feet)
            }
        ));
        assert!(matches!(spaced[1].kind, TokenKind::Minus));
        assert!(matches!(
            spaced[2].kind,
            TokenKind::Number {
                value: 6.0,
                unit: Some(Unit::Inches)
            }
        ));
    }

    #[test]
    fn numbers_and_exponents_are_lexed_without_stealing_identifiers() {
        assert!(
            matches!(kinds("1e-3")[0], TokenKind::Number { value, unit: None } if (value - 0.001).abs() < 1e-15)
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
}
