use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Command {
    Simple(SimpleCommand),
    Pipeline(Vec<Command>),
    And(Box<Command>, Box<Command>),
    Or(Box<Command>, Box<Command>),
    Not(Box<Command>),
    Sequence(Vec<Command>),
    If {
        condition: Box<Command>,
        then_branch: Box<Command>,
        elif_branches: Vec<(Command, Command)>,
        else_branch: Option<Box<Command>>,
    },
    For {
        var: String,
        words: Vec<Word>,
        body: Box<Command>,
    },
    While {
        condition: Box<Command>,
        body: Box<Command>,
    },
    Until {
        condition: Box<Command>,
        body: Box<Command>,
    },
    Case {
        word: Word,
        arms: Vec<CaseArm>,
    },
    FunctionDef {
        name: String,
        body: Box<Command>,
    },
    Subshell(Box<Command>),
    Group(Box<Command>),
    Assignment(Vec<Assignment>),
    Background(Box<Command>),
    Empty,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimpleCommand {
    pub words: Vec<Word>,
    pub redirections: Vec<Redirection>,
    pub assignments: Vec<Assignment>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Word {
    Literal(String),
    SingleQuoted(String),
    DoubleQuoted(Vec<WordPart>),
    Variable(String),
    CommandSub(Box<Command>),
    Compound(Vec<Word>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WordPart {
    Literal(String),
    Variable(String),
    CommandSub(Box<Command>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assignment {
    pub name: String,
    pub value: Word,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Redirection {
    pub fd: Option<u32>,
    pub kind: RedirectKind,
    pub target: Word,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RedirectKind {
    Input,      // <
    Output,     // >
    Append,     // >>
    ErrOutput,  // 2>
    ErrAppend,  // 2>>
    Both,       // &>
    HereDoc,    // <<
    HereString, // <<<
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaseArm {
    pub patterns: Vec<Word>,
    pub body: Command,
}
