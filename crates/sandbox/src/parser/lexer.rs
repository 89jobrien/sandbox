use crate::error::{ShellError, ShellResult};

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Word(String),
    SingleQuoted(String),
    DoubleQuoted(Vec<QuotedSegment>),
    Pipe,
    And,  // &&
    Or,   // ||
    Semi, // ;
    Newline,
    LParen,
    RParen,
    LBrace,                 // {
    RBrace,                 // }
    Bang,                   // !
    Assign(String, String), // name=value
    RedirectIn,             // <
    RedirectOut,            // >
    RedirectAppend,         // >>
    RedirectErr,            // 2>
    RedirectErrAppend,      // 2>>
    RedirectBoth,           // &>
    HereDoc(String),        // <<DELIM
    HereString(String),     // <<<
    Fd(u32),                // file descriptor prefix (e.g., 2 in 2>)
    Ampersand,              // & (background)
    DollarParen,            // $( for command substitution
    Backtick(String),       // `cmd`
    If,
    Then,
    Elif,
    Else,
    Fi,
    For,
    In,
    Do,
    Done,
    While,
    Until,
    Case,
    Esac,
    Function,
    Select,
    DoubleBracketOpen,  // [[
    DoubleBracketClose, // ]]
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub enum QuotedSegment {
    Literal(String),
    Variable(String),
    CommandSub(String),
}

pub struct Lexer {
    input: Vec<char>,
    pos: usize,
    fuel: usize,
    max_fuel: usize,
}

impl Lexer {
    pub fn new(input: &str, max_fuel: usize) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
            fuel: 0,
            max_fuel,
        }
    }

    fn burn_fuel(&mut self) -> ShellResult<()> {
        self.fuel += 1;
        if self.fuel > self.max_fuel {
            return Err(ShellError::ParserFuelExhausted(self.fuel));
        }
        Ok(())
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.input.get(self.pos + offset).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.input.get(self.pos).copied();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c == ' ' || c == '\t' {
                self.advance();
            } else if c == '#' {
                while let Some(c) = self.peek() {
                    if c == '\n' {
                        break;
                    }
                    self.advance();
                }
            } else {
                break;
            }
        }
    }

    pub fn tokenize(&mut self) -> ShellResult<Vec<Token>> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace();
            self.burn_fuel()?;
            match self.peek() {
                None => {
                    tokens.push(Token::Eof);
                    break;
                }
                Some('\n') => {
                    self.advance();
                    tokens.push(Token::Newline);
                }
                Some('|' | '&' | ';' | '(' | ')' | '{' | '}' | '!') => {
                    tokens.push(self.lex_operator());
                }
                Some('>' | '<') => {
                    tokens.extend(self.lex_redirect()?);
                }
                Some('\'') => {
                    let s = self.read_single_quoted()?;
                    tokens.push(Token::SingleQuoted(s));
                }
                Some('"') => {
                    let segs = self.read_double_quoted()?;
                    tokens.push(Token::DoubleQuoted(segs));
                }
                Some('$') => {
                    if self.peek_at(1) == Some('(') {
                        self.advance(); // $
                        self.advance(); // (
                        let cmd = self.read_until_balanced('(', ')')?;
                        tokens.push(Token::DollarParen);
                        tokens.push(Token::Word(cmd));
                        tokens.push(Token::RParen);
                    } else {
                        let w = self.read_word()?;
                        tokens.push(self.classify_word(w));
                    }
                }
                Some('`') => {
                    tokens.push(self.lex_backtick()?);
                }
                Some('2') if self.peek_at(1) == Some('>') => {
                    tokens.push(self.lex_stderr_redirect());
                }
                Some(_) => {
                    let w = self.read_word()?;
                    tokens.push(self.classify_word(w));
                }
            }
        }
        Ok(tokens)
    }

    fn lex_operator(&mut self) -> Token {
        match self.advance() {
            Some('|') if self.peek() == Some('|') => {
                self.advance();
                Token::Or
            }
            Some('|') => Token::Pipe,
            Some('&') if self.peek() == Some('&') => {
                self.advance();
                Token::And
            }
            Some('&') if self.peek() == Some('>') => {
                self.advance();
                Token::RedirectBoth
            }
            Some('&') => Token::Ampersand,
            Some(';') => Token::Semi,
            Some('(') => Token::LParen,
            Some(')') => Token::RParen,
            Some('{') => Token::LBrace,
            Some('}') => Token::RBrace,
            Some('!') => Token::Bang,
            _ => unreachable!(),
        }
    }

    fn lex_redirect(&mut self) -> ShellResult<Vec<Token>> {
        match self.advance() {
            Some('>') if self.peek() == Some('>') => {
                self.advance();
                Ok(vec![Token::RedirectAppend])
            }
            Some('>') => Ok(vec![Token::RedirectOut]),
            Some('<') if self.peek() == Some('<') => {
                self.advance();
                if self.peek() == Some('<') {
                    self.advance();
                    let s = self.read_here_string()?;
                    Ok(vec![Token::HereString(s)])
                } else {
                    let delim = self.read_here_doc_delim()?;
                    let body = self.read_here_doc_body(&delim)?;
                    Ok(vec![Token::HereDoc(body)])
                }
            }
            Some('<') => Ok(vec![Token::RedirectIn]),
            _ => unreachable!(),
        }
    }

    fn lex_backtick(&mut self) -> ShellResult<Token> {
        self.advance(); // opening `
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c == '`' {
                self.advance();
                break;
            }
            if c == '\\' {
                self.advance();
                if let Some(c2) = self.advance() {
                    s.push(c2);
                }
            } else {
                s.push(c);
                self.advance();
            }
        }
        Ok(Token::Backtick(s))
    }

    fn lex_stderr_redirect(&mut self) -> Token {
        self.advance(); // 2
        self.advance(); // >
        if self.peek() == Some('>') {
            self.advance();
            Token::RedirectErrAppend
        } else {
            Token::RedirectErr
        }
    }

    fn classify_word(&self, w: String) -> Token {
        if let Some(eq) = w.find('=') {
            let name = &w[..eq];
            if !name.is_empty()
                && name.chars().all(|c| c.is_alphanumeric() || c == '_')
                && name.starts_with(|c: char| c.is_alphabetic() || c == '_')
            {
                return Token::Assign(name.to_string(), w[eq + 1..].to_string());
            }
        }
        match w.as_str() {
            "if" => Token::If,
            "then" => Token::Then,
            "elif" => Token::Elif,
            "else" => Token::Else,
            "fi" => Token::Fi,
            "for" => Token::For,
            "in" => Token::In,
            "do" => Token::Do,
            "done" => Token::Done,
            "while" => Token::While,
            "until" => Token::Until,
            "case" => Token::Case,
            "esac" => Token::Esac,
            "function" => Token::Function,
            "select" => Token::Select,
            "[[" => Token::DoubleBracketOpen,
            "]]" => Token::DoubleBracketClose,
            _ => Token::Word(w),
        }
    }

    fn read_word(&mut self) -> ShellResult<String> {
        let mut word = String::new();
        while let Some(c) = self.peek() {
            self.burn_fuel()?;
            match c {
                ' ' | '\t' | '\n' | '|' | '&' | ';' | '(' | ')' | '<' | '>' | '{' | '}' => break,
                '\'' => {
                    let s = self.read_single_quoted()?;
                    word.push_str(&s);
                }
                '"' => {
                    let segs = self.read_double_quoted()?;
                    for seg in segs {
                        match seg {
                            QuotedSegment::Literal(s) => word.push_str(&s),
                            QuotedSegment::Variable(v) => {
                                word.push('$');
                                word.push_str(&v);
                            }
                            QuotedSegment::CommandSub(c) => {
                                word.push_str("$(");
                                word.push_str(&c);
                                word.push(')');
                            }
                        }
                    }
                }
                '\\' => {
                    self.advance();
                    if let Some(c2) = self.advance() {
                        word.push(c2);
                    }
                }
                _ => {
                    word.push(c);
                    self.advance();
                }
            }
        }
        Ok(word)
    }

    fn read_single_quoted(&mut self) -> ShellResult<String> {
        self.advance(); // opening '
        let mut s = String::new();
        loop {
            self.burn_fuel()?;
            match self.advance() {
                Some('\'') => break,
                Some(c) => s.push(c),
                None => return Err(ShellError::ParseError("unterminated single quote".into())),
            }
        }
        Ok(s)
    }

    fn read_double_quoted(&mut self) -> ShellResult<Vec<QuotedSegment>> {
        self.advance(); // opening "
        let mut segments = Vec::new();
        let mut literal = String::new();
        loop {
            self.burn_fuel()?;
            match self.peek() {
                None => return Err(ShellError::ParseError("unterminated double quote".into())),
                Some('"') => {
                    self.advance();
                    if !literal.is_empty() {
                        segments.push(QuotedSegment::Literal(literal));
                    }
                    break;
                }
                Some('\\') => {
                    self.advance();
                    if let Some(c) = self.advance() {
                        literal.push(c);
                    }
                }
                Some('$') => {
                    if !literal.is_empty() {
                        segments.push(QuotedSegment::Literal(std::mem::take(&mut literal)));
                    }
                    self.advance(); // $
                    if self.peek() == Some('(') {
                        self.advance(); // (
                        let cmd = self.read_until_balanced('(', ')')?;
                        segments.push(QuotedSegment::CommandSub(cmd));
                    } else if self.peek() == Some('{') {
                        self.advance(); // {
                        let var = self.read_until_char('}')?;
                        self.advance(); // }
                        segments.push(QuotedSegment::Variable(var));
                    } else {
                        let var = self.read_var_name();
                        if var.is_empty() {
                            literal.push('$');
                        } else {
                            segments.push(QuotedSegment::Variable(var));
                        }
                    }
                }
                Some('`') => {
                    if !literal.is_empty() {
                        segments.push(QuotedSegment::Literal(std::mem::take(&mut literal)));
                    }
                    self.advance(); // `
                    let mut cmd = String::new();
                    loop {
                        match self.advance() {
                            Some('`') => break,
                            Some(c) => cmd.push(c),
                            None => {
                                return Err(ShellError::ParseError("unterminated backtick".into()));
                            }
                        }
                    }
                    segments.push(QuotedSegment::CommandSub(cmd));
                }
                Some(c) => {
                    literal.push(c);
                    self.advance();
                }
            }
        }
        Ok(segments)
    }

    fn read_var_name(&mut self) -> String {
        let mut name = String::new();
        // Special variables
        if let Some(c) = self.peek()
            && "?$!#@*-0123456789".contains(c)
        {
            name.push(c);
            self.advance();
            return name;
        }
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                name.push(c);
                self.advance();
            } else {
                break;
            }
        }
        name
    }

    fn read_until_balanced(&mut self, open: char, close: char) -> ShellResult<String> {
        let mut depth = 1;
        let mut s = String::new();
        while depth > 0 {
            self.burn_fuel()?;
            match self.advance() {
                Some(c) if c == open => {
                    depth += 1;
                    s.push(c);
                }
                Some(c) if c == close => {
                    depth -= 1;
                    if depth > 0 {
                        s.push(c);
                    }
                }
                Some(c) => s.push(c),
                None => {
                    return Err(ShellError::ParseError(format!(
                        "unterminated '{open}'..'{close}'"
                    )));
                }
            }
        }
        Ok(s)
    }

    fn read_until_char(&mut self, end: char) -> ShellResult<String> {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            self.burn_fuel()?;
            if c == end {
                return Ok(s);
            }
            s.push(c);
            self.advance();
        }
        Err(ShellError::ParseError(format!(
            "expected '{end}' not found"
        )))
    }

    fn read_here_doc_delim(&mut self) -> ShellResult<String> {
        self.skip_whitespace();
        let mut delim = String::new();
        while let Some(c) = self.peek() {
            if c == '\n' || c == ' ' || c == '\t' {
                break;
            }
            delim.push(c);
            self.advance();
        }
        if delim.is_empty() {
            return Err(ShellError::ParseError("empty here-doc delimiter".into()));
        }
        // Strip quotes from delimiter
        let delim = delim.trim_matches(|c| c == '\'' || c == '"').to_string();
        Ok(delim)
    }

    fn read_here_doc_body(&mut self, delim: &str) -> ShellResult<String> {
        // Skip the newline after the delimiter
        if self.peek() == Some('\n') {
            self.advance();
        }
        let mut body = String::new();
        let mut line = String::new();
        loop {
            self.burn_fuel()?;
            match self.advance() {
                None => break,
                Some('\n') => {
                    let trimmed = line.trim_start_matches('\t');
                    if trimmed == delim {
                        break;
                    }
                    body.push_str(&line);
                    body.push('\n');
                    line.clear();
                }
                Some(c) => line.push(c),
            }
        }
        Ok(body)
    }

    fn read_here_string(&mut self) -> ShellResult<String> {
        self.skip_whitespace();
        if self.peek() == Some('\'') {
            self.read_single_quoted()
        } else if self.peek() == Some('"') {
            let segs = self.read_double_quoted()?;
            Ok(segs
                .into_iter()
                .map(|s| match s {
                    QuotedSegment::Literal(l) => l,
                    QuotedSegment::Variable(v) => format!("${v}"),
                    QuotedSegment::CommandSub(c) => format!("$({c})"),
                })
                .collect())
        } else {
            self.read_word()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(input: &str) -> Vec<Token> {
        Lexer::new(input, 10_000).tokenize().unwrap()
    }

    #[test]
    fn simple_command() {
        let tokens = lex("echo hello");
        assert_eq!(
            tokens,
            vec![
                Token::Word("echo".into()),
                Token::Word("hello".into()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn pipe() {
        let tokens = lex("cat file | grep foo");
        assert_eq!(
            tokens,
            vec![
                Token::Word("cat".into()),
                Token::Word("file".into()),
                Token::Pipe,
                Token::Word("grep".into()),
                Token::Word("foo".into()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn assignment() {
        let tokens = lex("FOO=bar");
        assert_eq!(
            tokens,
            vec![Token::Assign("FOO".into(), "bar".into()), Token::Eof]
        );
    }

    #[test]
    fn single_quoted() {
        let tokens = lex("echo 'hello world'");
        assert_eq!(
            tokens,
            vec![
                Token::Word("echo".into()),
                Token::SingleQuoted("hello world".into()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn keywords() {
        let tokens = lex("if true; then echo yes; fi");
        assert!(matches!(tokens[0], Token::If));
        assert!(matches!(tokens[1], Token::Word(ref s) if s == "true"));
        assert!(matches!(tokens[2], Token::Semi));
        assert!(matches!(tokens[3], Token::Then));
    }

    #[test]
    fn redirect() {
        let tokens = lex("echo hello > file.txt");
        assert!(tokens.contains(&Token::RedirectOut));
    }

    #[test]
    fn and_or() {
        let tokens = lex("true && false || echo fail");
        assert!(tokens.contains(&Token::And));
        assert!(tokens.contains(&Token::Or));
    }

    #[test]
    fn fuel_exhaustion() {
        let result = Lexer::new("echo hello", 1).tokenize();
        assert!(result.is_err());
    }
}
