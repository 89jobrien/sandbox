pub mod ast;
pub mod lexer;

use ast::*;
use lexer::{QuotedSegment, Token};

use crate::error::{ShellError, ShellResult};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    depth: usize,
    max_depth: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>, max_depth: usize) -> Self {
        Self {
            tokens,
            pos: 0,
            depth: 0,
            max_depth,
        }
    }

    pub fn parse(input: &str, max_fuel: usize, max_depth: usize) -> ShellResult<Command> {
        let tokens = lexer::Lexer::new(input, max_fuel).tokenize()?;
        let mut parser = Self::new(tokens, max_depth);
        parser.parse_program()
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) -> &Token {
        let tok = self.tokens.get(self.pos).unwrap_or(&Token::Eof);
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn eat(&mut self, expected: &Token) -> ShellResult<()> {
        let got = self.advance().clone();
        if std::mem::discriminant(&got) != std::mem::discriminant(expected) {
            return Err(ShellError::ParseError(format!(
                "expected {expected:?}, got {got:?}"
            )));
        }
        Ok(())
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Token::Newline) {
            self.advance();
        }
    }

    fn enter_depth(&mut self) -> ShellResult<()> {
        self.depth += 1;
        if self.depth > self.max_depth {
            return Err(ShellError::MaxDepthExceeded(self.depth));
        }
        Ok(())
    }

    fn exit_depth(&mut self) {
        self.depth -= 1;
    }

    fn parse_program(&mut self) -> ShellResult<Command> {
        self.skip_newlines();
        if matches!(self.peek(), Token::Eof) {
            return Ok(Command::Empty);
        }
        let cmd = self.parse_list()?;
        self.skip_newlines();
        Ok(cmd)
    }

    fn parse_list(&mut self) -> ShellResult<Command> {
        self.enter_depth()?;
        let first = self.parse_and_or()?;
        let mut cmds = vec![first];

        loop {
            match self.peek() {
                Token::Semi | Token::Newline => {
                    self.advance();
                    self.skip_newlines();
                    if matches!(
                        self.peek(),
                        Token::Eof
                            | Token::Fi
                            | Token::Done
                            | Token::Esac
                            | Token::RBrace
                            | Token::RParen
                            | Token::Else
                            | Token::Elif
                            | Token::Then
                            | Token::Do
                    ) {
                        break;
                    }
                    cmds.push(self.parse_and_or()?);
                }
                _ => break,
            }
        }

        self.exit_depth();
        if cmds.len() == 1 {
            Ok(cmds.into_iter().next().unwrap())
        } else {
            Ok(Command::Sequence(cmds))
        }
    }

    fn parse_and_or(&mut self) -> ShellResult<Command> {
        let mut left = self.parse_pipeline()?;
        loop {
            match self.peek() {
                Token::And => {
                    self.advance();
                    self.skip_newlines();
                    let right = self.parse_pipeline()?;
                    left = Command::And(Box::new(left), Box::new(right));
                }
                Token::Or => {
                    self.advance();
                    self.skip_newlines();
                    let right = self.parse_pipeline()?;
                    left = Command::Or(Box::new(left), Box::new(right));
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_pipeline(&mut self) -> ShellResult<Command> {
        let negate = if matches!(self.peek(), Token::Bang) {
            self.advance();
            true
        } else {
            false
        };

        let first = self.parse_command()?;
        let mut cmds = vec![first];

        while matches!(self.peek(), Token::Pipe) {
            self.advance();
            self.skip_newlines();
            cmds.push(self.parse_command()?);
        }

        let cmd = if cmds.len() == 1 {
            cmds.into_iter().next().unwrap()
        } else {
            Command::Pipeline(cmds)
        };

        if negate {
            Ok(Command::Not(Box::new(cmd)))
        } else {
            Ok(cmd)
        }
    }

    fn parse_command(&mut self) -> ShellResult<Command> {
        self.enter_depth()?;
        let cmd = match self.peek() {
            Token::If => self.parse_if()?,
            Token::For => self.parse_for()?,
            Token::While => self.parse_while()?,
            Token::Until => self.parse_until()?,
            Token::Case => self.parse_case()?,
            Token::Function => self.parse_function_def()?,
            Token::LParen => {
                self.advance();
                self.skip_newlines();
                let inner = self.parse_list()?;
                self.eat(&Token::RParen)?;
                Command::Subshell(Box::new(inner))
            }
            Token::LBrace => {
                self.advance();
                self.skip_newlines();
                let inner = self.parse_list()?;
                self.skip_newlines();
                self.eat(&Token::RBrace)?;
                Command::Group(Box::new(inner))
            }
            _ => self.parse_simple_command()?,
        };
        self.exit_depth();
        Ok(cmd)
    }

    fn parse_simple_command(&mut self) -> ShellResult<Command> {
        let mut words = Vec::new();
        let mut redirections = Vec::new();
        let mut assignments = Vec::new();

        // Leading assignments
        while let Token::Assign(_, _) = self.peek() {
            if let Token::Assign(name, value) = self.advance().clone() {
                assignments.push(Assignment {
                    name,
                    value: Word::Literal(value),
                });
            }
        }

        loop {
            match self.peek() {
                Token::Word(w) => {
                    let w = w.clone();
                    self.advance();
                    // Check if this word followed by '(' is a function def
                    if words.is_empty() && matches!(self.peek(), Token::LParen) {
                        // Could be function def: name() { ... }
                        self.advance(); // (
                        self.eat(&Token::RParen)?;
                        self.skip_newlines();
                        let body = self.parse_command()?;
                        return Ok(Command::FunctionDef {
                            name: w,
                            body: Box::new(body),
                        });
                    }
                    words.push(Word::Literal(w));
                }
                Token::SingleQuoted(s) => {
                    let s = s.clone();
                    self.advance();
                    words.push(Word::SingleQuoted(s));
                }
                Token::DoubleQuoted(segs) => {
                    let segs = segs.clone();
                    self.advance();
                    let parts = segs
                        .into_iter()
                        .map(|s| match s {
                            QuotedSegment::Literal(l) => WordPart::Literal(l),
                            QuotedSegment::Variable(v) => WordPart::Variable(v),
                            QuotedSegment::CommandSub(c) => {
                                // Parse the command substitution body
                                match Parser::parse(&c, 10_000, 50) {
                                    Ok(cmd) => WordPart::CommandSub(Box::new(cmd)),
                                    Err(_) => WordPart::Literal(format!("$({c})")),
                                }
                            }
                        })
                        .collect();
                    words.push(Word::DoubleQuoted(parts));
                }
                Token::DollarParen => {
                    self.advance(); // DollarParen already consumed
                    if let Token::Word(cmd_str) = self.peek().clone() {
                        let cmd_str = cmd_str.clone();
                        self.advance();
                        if matches!(self.peek(), Token::RParen) {
                            self.advance();
                        }
                        match Parser::parse(&cmd_str, 10_000, 50) {
                            Ok(cmd) => words.push(Word::CommandSub(Box::new(cmd))),
                            Err(_) => words.push(Word::Literal(format!("$({cmd_str})"))),
                        }
                    }
                }
                Token::Backtick(cmd_str) => {
                    let cmd_str = cmd_str.clone();
                    self.advance();
                    match Parser::parse(&cmd_str, 10_000, 50) {
                        Ok(cmd) => words.push(Word::CommandSub(Box::new(cmd))),
                        Err(_) => words.push(Word::Literal(cmd_str)),
                    }
                }
                Token::RedirectOut => {
                    self.advance();
                    let target = self.parse_word()?;
                    redirections.push(Redirection {
                        fd: Some(1),
                        kind: RedirectKind::Output,
                        target,
                    });
                }
                Token::RedirectAppend => {
                    self.advance();
                    let target = self.parse_word()?;
                    redirections.push(Redirection {
                        fd: Some(1),
                        kind: RedirectKind::Append,
                        target,
                    });
                }
                Token::RedirectIn => {
                    self.advance();
                    let target = self.parse_word()?;
                    redirections.push(Redirection {
                        fd: Some(0),
                        kind: RedirectKind::Input,
                        target,
                    });
                }
                Token::RedirectErr => {
                    self.advance();
                    let target = self.parse_word()?;
                    redirections.push(Redirection {
                        fd: Some(2),
                        kind: RedirectKind::ErrOutput,
                        target,
                    });
                }
                Token::RedirectErrAppend => {
                    self.advance();
                    let target = self.parse_word()?;
                    redirections.push(Redirection {
                        fd: Some(2),
                        kind: RedirectKind::ErrAppend,
                        target,
                    });
                }
                Token::RedirectBoth => {
                    self.advance();
                    let target = self.parse_word()?;
                    redirections.push(Redirection {
                        fd: None,
                        kind: RedirectKind::Both,
                        target,
                    });
                }
                Token::HereDoc(body) => {
                    let body = body.clone();
                    self.advance();
                    redirections.push(Redirection {
                        fd: Some(0),
                        kind: RedirectKind::HereDoc,
                        target: Word::Literal(body),
                    });
                }
                Token::HereString(s) => {
                    let s = s.clone();
                    self.advance();
                    redirections.push(Redirection {
                        fd: Some(0),
                        kind: RedirectKind::HereString,
                        target: Word::Literal(s),
                    });
                }
                Token::Assign(name, value) => {
                    let name = name.clone();
                    let value = value.clone();
                    self.advance();
                    if words.is_empty() {
                        assignments.push(Assignment {
                            name,
                            value: Word::Literal(value),
                        });
                    } else {
                        words.push(Word::Literal(format!("{name}={value}")));
                    }
                }
                _ => break,
            }
        }

        if words.is_empty() && !assignments.is_empty() && redirections.is_empty() {
            return Ok(Command::Assignment(assignments));
        }

        Ok(Command::Simple(SimpleCommand {
            words,
            redirections,
            assignments,
        }))
    }

    fn parse_word(&mut self) -> ShellResult<Word> {
        match self.peek() {
            Token::Word(w) => {
                let w = w.clone();
                self.advance();
                Ok(Word::Literal(w))
            }
            Token::SingleQuoted(s) => {
                let s = s.clone();
                self.advance();
                Ok(Word::SingleQuoted(s))
            }
            Token::DoubleQuoted(segs) => {
                let segs = segs.clone();
                self.advance();
                let parts = segs
                    .into_iter()
                    .map(|s| match s {
                        QuotedSegment::Literal(l) => WordPart::Literal(l),
                        QuotedSegment::Variable(v) => WordPart::Variable(v),
                        QuotedSegment::CommandSub(c) => match Parser::parse(&c, 10_000, 50) {
                            Ok(cmd) => WordPart::CommandSub(Box::new(cmd)),
                            Err(_) => WordPart::Literal(format!("$({c})")),
                        },
                    })
                    .collect();
                Ok(Word::DoubleQuoted(parts))
            }
            other => Err(ShellError::ParseError(format!(
                "expected word, got {other:?}"
            ))),
        }
    }

    fn parse_if(&mut self) -> ShellResult<Command> {
        self.eat(&Token::If)?;
        self.skip_newlines();
        let condition = self.parse_list()?;
        self.skip_newlines();
        self.eat(&Token::Then)?;
        self.skip_newlines();
        let then_branch = self.parse_list()?;
        self.skip_newlines();

        let mut elif_branches = Vec::new();
        while matches!(self.peek(), Token::Elif) {
            self.advance();
            self.skip_newlines();
            let cond = self.parse_list()?;
            self.skip_newlines();
            self.eat(&Token::Then)?;
            self.skip_newlines();
            let body = self.parse_list()?;
            self.skip_newlines();
            elif_branches.push((cond, body));
        }

        let else_branch = if matches!(self.peek(), Token::Else) {
            self.advance();
            self.skip_newlines();
            Some(Box::new(self.parse_list()?))
        } else {
            None
        };
        self.skip_newlines();
        self.eat(&Token::Fi)?;

        Ok(Command::If {
            condition: Box::new(condition),
            then_branch: Box::new(then_branch),
            elif_branches,
            else_branch,
        })
    }

    fn parse_for(&mut self) -> ShellResult<Command> {
        self.eat(&Token::For)?;
        let var = match self.advance().clone() {
            Token::Word(w) => w,
            other => {
                return Err(ShellError::ParseError(format!(
                    "expected variable name after 'for', got {other:?}"
                )));
            }
        };
        self.skip_newlines();

        let words = if matches!(self.peek(), Token::In) {
            self.advance();
            let mut ws = Vec::new();
            loop {
                match self.peek() {
                    Token::Semi | Token::Newline | Token::Do => break,
                    Token::Eof => break,
                    _ => ws.push(self.parse_word()?),
                }
            }
            ws
        } else {
            Vec::new()
        };

        // Skip separator
        match self.peek() {
            Token::Semi | Token::Newline => {
                self.advance();
            }
            _ => {}
        }
        self.skip_newlines();
        self.eat(&Token::Do)?;
        self.skip_newlines();
        let body = self.parse_list()?;
        self.skip_newlines();
        self.eat(&Token::Done)?;

        Ok(Command::For {
            var,
            words,
            body: Box::new(body),
        })
    }

    fn parse_while(&mut self) -> ShellResult<Command> {
        self.eat(&Token::While)?;
        self.skip_newlines();
        let condition = self.parse_list()?;
        self.skip_newlines();
        self.eat(&Token::Do)?;
        self.skip_newlines();
        let body = self.parse_list()?;
        self.skip_newlines();
        self.eat(&Token::Done)?;
        Ok(Command::While {
            condition: Box::new(condition),
            body: Box::new(body),
        })
    }

    fn parse_until(&mut self) -> ShellResult<Command> {
        self.eat(&Token::Until)?;
        self.skip_newlines();
        let condition = self.parse_list()?;
        self.skip_newlines();
        self.eat(&Token::Do)?;
        self.skip_newlines();
        let body = self.parse_list()?;
        self.skip_newlines();
        self.eat(&Token::Done)?;
        Ok(Command::Until {
            condition: Box::new(condition),
            body: Box::new(body),
        })
    }

    fn parse_case(&mut self) -> ShellResult<Command> {
        self.eat(&Token::Case)?;
        let word = self.parse_word()?;
        self.skip_newlines();
        self.eat(&Token::In)?;
        self.skip_newlines();

        let mut arms = Vec::new();
        while !matches!(self.peek(), Token::Esac | Token::Eof) {
            // Parse pattern(s)
            let mut patterns = vec![self.parse_word()?];
            while matches!(self.peek(), Token::Pipe) {
                self.advance();
                patterns.push(self.parse_word()?);
            }
            self.eat(&Token::RParen)?;
            self.skip_newlines();

            let body = if matches!(self.peek(), Token::Semi | Token::Esac | Token::Eof) {
                Command::Empty
            } else {
                self.parse_list()?
            };
            self.skip_newlines();

            // ;; terminator
            if matches!(self.peek(), Token::Semi) {
                self.advance();
                if matches!(self.peek(), Token::Semi) {
                    self.advance();
                }
            }
            self.skip_newlines();

            arms.push(CaseArm { patterns, body });
        }
        self.eat(&Token::Esac)?;
        Ok(Command::Case { word, arms })
    }

    fn parse_function_def(&mut self) -> ShellResult<Command> {
        self.eat(&Token::Function)?;
        let name = match self.advance().clone() {
            Token::Word(w) => w,
            other => {
                return Err(ShellError::ParseError(format!(
                    "expected function name, got {other:?}"
                )));
            }
        };
        // Optional ()
        if matches!(self.peek(), Token::LParen) {
            self.advance();
            self.eat(&Token::RParen)?;
        }
        self.skip_newlines();
        let body = self.parse_command()?;
        Ok(Command::FunctionDef {
            name,
            body: Box::new(body),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> Command {
        Parser::parse(input, 100_000, 100).unwrap()
    }

    #[test]
    fn simple_echo() {
        let cmd = parse("echo hello world");
        match cmd {
            Command::Simple(sc) => {
                assert_eq!(sc.words.len(), 3);
            }
            _ => panic!("expected simple command"),
        }
    }

    #[test]
    fn pipeline() {
        let cmd = parse("echo hello | cat");
        assert!(matches!(cmd, Command::Pipeline(_)));
    }

    #[test]
    fn and_or() {
        let cmd = parse("true && echo yes || echo no");
        assert!(matches!(cmd, Command::Or(_, _)));
    }

    #[test]
    fn sequence() {
        let cmd = parse("echo a; echo b; echo c");
        assert!(matches!(cmd, Command::Sequence(_)));
    }

    #[test]
    fn if_then_fi() {
        let cmd = parse("if true; then echo yes; fi");
        assert!(matches!(cmd, Command::If { .. }));
    }

    #[test]
    fn for_loop() {
        let cmd = parse("for i in 1 2 3; do echo $i; done");
        assert!(matches!(cmd, Command::For { .. }));
    }

    #[test]
    fn while_loop() {
        let cmd = parse("while true; do echo loop; done");
        assert!(matches!(cmd, Command::While { .. }));
    }

    #[test]
    fn assignment() {
        let cmd = parse("FOO=bar");
        assert!(matches!(cmd, Command::Assignment(_)));
    }

    #[test]
    fn redirect() {
        let cmd = parse("echo hello > output.txt");
        match cmd {
            Command::Simple(sc) => {
                assert_eq!(sc.redirections.len(), 1);
                assert_eq!(sc.redirections[0].kind, RedirectKind::Output);
            }
            _ => panic!("expected simple command"),
        }
    }

    #[test]
    fn function_def() {
        let cmd = parse("function greet { echo hello; }");
        assert!(matches!(cmd, Command::FunctionDef { .. }));
    }

    #[test]
    fn depth_exceeded() {
        // Deeply nested subshells
        let input = "(".repeat(200) + &")".repeat(200);
        let result = Parser::parse(&input, 100_000, 100);
        assert!(result.is_err());
    }
}
