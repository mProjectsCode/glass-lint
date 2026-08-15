use swc_ecma_ast::EsVersion;
use swc_ecma_parser::{
    StringInput, Syntax,
    lexer::Lexer,
    unstable::{Token, TokenAndSpan},
};

use super::{SyntaxDepthError, SyntaxDepthOutcome};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Delimiter {
    Parenthesis,
    Bracket,
    Brace,
}

/// Owns the mutable state used while measuring syntactic nesting.
///
/// The source and post-parse scans share delimiter, member-chain, and
/// expression-ending state, while only the source scan needs regex recovery.
pub(super) struct DepthScanner {
    delimiters: Vec<Delimiter>,
    depth: usize,
    maximum: usize,
    member_depth: usize,
    expression_can_end: bool,
    previous: Option<Token>,
    previous_postfix: bool,
    max_depth: usize,
}

impl DepthScanner {
    pub(super) fn new(max_depth: usize) -> Self {
        Self {
            delimiters: Vec::new(),
            depth: 0,
            maximum: 0,
            member_depth: 0,
            expression_can_end: false,
            previous: None,
            previous_postfix: false,
            max_depth,
        }
    }

    pub(super) fn scan_source(
        &mut self,
        file: &swc_common::SourceFile,
        syntax: Syntax,
    ) -> SyntaxDepthOutcome {
        let mut lexer = Lexer::new(syntax, EsVersion::EsNext, StringInput::from(file), None);
        let mut skip_to = 0usize;
        self.scan(&mut lexer, |scanner, token_and_span| {
            scanner.source_token(file, &token_and_span, &mut skip_to)
        })
    }

    pub(super) fn scan_tokens(&mut self, tokens: &[TokenAndSpan]) -> SyntaxDepthOutcome {
        self.scan(tokens.iter(), |_, token_and_span| {
            Some(token_and_span.token)
        })
    }

    fn scan<I, F>(&mut self, tokens: I, mut token_for: F) -> SyntaxDepthOutcome
    where
        I: IntoIterator,
        F: FnMut(&mut Self, I::Item) -> Option<Token>,
    {
        for token_item in tokens {
            let Some(token) = token_for(self, token_item) else {
                continue;
            };
            if token == Token::Error {
                break;
            }
            if self.observe(token).is_err() {
                return SyntaxDepthOutcome::Exceeded;
            }
        }
        SyntaxDepthOutcome::WithinLimit(self.maximum)
    }

    pub(super) fn source_token(
        &mut self,
        file: &swc_common::SourceFile,
        token_and_span: &TokenAndSpan,
        skip_to: &mut usize,
    ) -> Option<Token> {
        let offset = token_and_span
            .span
            .lo
            .0
            .checked_sub(file.start_pos.0)
            .map_or(0, |value| value as usize);
        if offset < *skip_to {
            return None;
        }

        let token = token_and_span.token;
        if token == Token::Slash
            && !self.previous_postfix
            && self
                .previous
                .is_none_or(|previous| previous.before_expr() || previous == Token::LBrace)
            && let Some(end) = Self::regex_end(&file.src, offset + 1)
        {
            *skip_to = end;
            self.previous = Some(Token::Regex);
            self.previous_postfix = false;
            self.expression_can_end = true;
            return None;
        }
        Some(token)
    }

    pub(super) fn observe(&mut self, token: Token) -> Result<(), SyntaxDepthError> {
        match token {
            Token::LParen => self.push_delimiter(Delimiter::Parenthesis)?,
            Token::LBracket => self.push_delimiter(Delimiter::Bracket)?,
            Token::LBrace | Token::DollarLBrace | Token::TemplateHead => {
                self.push_delimiter(Delimiter::Brace)?;
            }
            Token::RParen => self.pop_delimiter(Delimiter::Parenthesis),
            Token::RBracket => self.pop_delimiter(Delimiter::Bracket),
            Token::RBrace => self.pop_delimiter(Delimiter::Brace),
            Token::Dot | Token::OptionalChain => {
                self.member_depth = self.member_depth.saturating_add(1);
                self.maximum = self.maximum.max(self.member_depth);
            }
            token if Self::resets_member_depth(token) => self.member_depth = 0,
            _ => {}
        }
        self.previous_postfix =
            matches!(token, Token::PlusPlus | Token::MinusMinus) && self.expression_can_end;
        self.expression_can_end = Self::token_can_end_expression(token, self.previous_postfix);
        self.previous = Some(token);
        Ok(())
    }

    fn push_delimiter(&mut self, delimiter: Delimiter) -> Result<(), SyntaxDepthError> {
        self.depth = self.depth.saturating_add(1);
        self.maximum = self.maximum.max(self.depth);
        if self.maximum > self.max_depth {
            return Err(SyntaxDepthError::Exceeded);
        }
        self.delimiters.push(delimiter);
        Ok(())
    }

    fn pop_delimiter(&mut self, expected: Delimiter) {
        if self.delimiters.last() != Some(&expected) {
            return;
        }
        self.delimiters.pop();
        self.depth = self.depth.saturating_sub(1);
    }

    /// A conservative source-only upper bound used to decide whether a parser
    /// token stream is safe to inspect after parsing. Every delimiter and
    /// member separator that can increase tracked depth is counted, including
    /// ones inside literals and comments.
    pub(super) fn raw_bound(source: &str) -> usize {
        source
            .bytes()
            .filter(|byte| matches!(byte, b'(' | b'[' | b'{' | b'.'))
            .count()
    }

    pub(super) fn regex_end(source: &str, start: usize) -> Option<usize> {
        let bytes = source.as_bytes();
        let mut index = start;
        let mut in_character_class = false;
        while index < bytes.len() {
            match bytes[index] {
                b'\\' => index = index.checked_add(2)?,
                b'[' if !in_character_class => {
                    in_character_class = true;
                    index += 1;
                }
                b']' if in_character_class => {
                    in_character_class = false;
                    index += 1;
                }
                b'/' if !in_character_class => return Some(index + 1),
                b'\n' | b'\r' => return None,
                _ => index += 1,
            }
        }
        None
    }

    pub(super) fn resets_member_depth(token: Token) -> bool {
        matches!(
            token,
            Token::Semi
                | Token::Comma
                | Token::Colon
                | Token::Bang
                | Token::Plus
                | Token::Minus
                | Token::Asterisk
                | Token::Slash
                | Token::Percent
                | Token::Lt
                | Token::Gt
                | Token::Pipe
                | Token::Caret
                | Token::Ampersand
                | Token::Eq
                | Token::PlusPlus
                | Token::MinusMinus
                | Token::Tilde
                | Token::DotDotDot
        ) || token.is_bin_op()
            || token.is_assign_op()
    }

    pub(super) fn token_can_end_expression(token: Token, postfix: bool) -> bool {
        postfix
            || matches!(
                token,
                Token::Ident
                    | Token::Str
                    | Token::Num
                    | Token::BigInt
                    | Token::Regex
                    | Token::NoSubstitutionTemplateLiteral
                    | Token::TemplateTail
                    | Token::RParen
                    | Token::RBracket
                    | Token::RBrace
                    | Token::Null
                    | Token::True
                    | Token::False
                    | Token::This
                    | Token::Super
            )
    }
}
