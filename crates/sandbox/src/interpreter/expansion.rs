use std::collections::HashMap;

use crate::parser::ast::{Word, WordPart};

pub struct ExpansionContext<'a> {
    pub env: &'a HashMap<String, String>,
    pub vars: &'a HashMap<String, String>,
    pub last_exit_code: i32,
    pub positional_params: &'a [String],
}

impl ExpansionContext<'_> {
    pub fn expand_word(&self, word: &Word) -> String {
        match word {
            Word::Literal(s) => self.expand_variables(s),
            Word::SingleQuoted(s) => s.clone(),
            Word::DoubleQuoted(parts) => parts
                .iter()
                .map(|p| match p {
                    WordPart::Literal(s) => s.clone(),
                    WordPart::Variable(v) => self.lookup_var(v),
                    WordPart::CommandSub(_) => String::new(), // handled by interpreter
                })
                .collect(),
            Word::Variable(name) => self.lookup_var(name),
            Word::CommandSub(_) => String::new(), // handled by interpreter
            Word::Compound(parts) => parts.iter().map(|w| self.expand_word(w)).collect(),
        }
    }

    pub fn expand_words(&self, words: &[Word]) -> Vec<String> {
        words.iter().map(|w| self.expand_word(w)).collect()
    }

    fn expand_variables(&self, input: &str) -> String {
        let mut result = String::new();
        let chars: Vec<char> = input.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            if chars[i] == '$' {
                i += 1;
                if i >= chars.len() {
                    result.push('$');
                    break;
                }
                if chars[i] == '{' {
                    // ${VAR} form
                    i += 1;
                    let start = i;
                    while i < chars.len() && chars[i] != '}' {
                        i += 1;
                    }
                    let name: String = chars[start..i].iter().collect();
                    if i < chars.len() {
                        i += 1; // skip }
                    }
                    result.push_str(&self.lookup_var(&name));
                } else if chars[i] == '(' {
                    // $(...) — skip, handled at higher level
                    result.push('$');
                    result.push('(');
                    i += 1;
                } else {
                    // $VAR form
                    let var = self.read_var_name(&chars, &mut i);
                    result.push_str(&self.lookup_var(&var));
                }
            } else {
                result.push(chars[i]);
                i += 1;
            }
        }
        result
    }

    fn read_var_name(&self, chars: &[char], i: &mut usize) -> String {
        // Special single-char variables
        if *i < chars.len() && "?$!#@*-0123456789".contains(chars[*i]) {
            let c = chars[*i];
            *i += 1;
            return c.to_string();
        }

        let start = *i;
        while *i < chars.len() && (chars[*i].is_alphanumeric() || chars[*i] == '_') {
            *i += 1;
        }
        chars[start..*i].iter().collect()
    }

    fn lookup_var(&self, name: &str) -> String {
        match name {
            "?" => self.last_exit_code.to_string(),
            "#" => self.positional_params.len().to_string(),
            "@" | "*" => self.positional_params.join(" "),
            n if n.parse::<usize>().is_ok() => {
                let idx: usize = n.parse().unwrap();
                self.positional_params
                    .get(idx.wrapping_sub(1))
                    .cloned()
                    .unwrap_or_default()
            }
            _ => self
                .vars
                .get(name)
                .or_else(|| self.env.get(name))
                .cloned()
                .unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(
        env: &'a HashMap<String, String>,
        vars: &'a HashMap<String, String>,
    ) -> ExpansionContext<'a> {
        ExpansionContext {
            env,
            vars,
            last_exit_code: 0,
            positional_params: &[],
        }
    }

    #[test]
    fn expand_simple_var() {
        let mut env = HashMap::new();
        env.insert("HOME".into(), "/home/user".into());
        let vars = HashMap::new();
        let c = ctx(&env, &vars);
        assert_eq!(c.expand_variables("$HOME"), "/home/user");
    }

    #[test]
    fn expand_braced_var() {
        let mut env = HashMap::new();
        env.insert("NAME".into(), "world".into());
        let vars = HashMap::new();
        let c = ctx(&env, &vars);
        assert_eq!(c.expand_variables("hello ${NAME}!"), "hello world!");
    }

    #[test]
    fn expand_exit_code() {
        let env = HashMap::new();
        let vars = HashMap::new();
        let c = ExpansionContext {
            env: &env,
            vars: &vars,
            last_exit_code: 42,
            positional_params: &[],
        };
        assert_eq!(c.expand_variables("$?"), "42");
    }

    #[test]
    fn single_quoted_no_expansion() {
        let mut env = HashMap::new();
        env.insert("X".into(), "val".into());
        let vars = HashMap::new();
        let c = ctx(&env, &vars);
        assert_eq!(c.expand_word(&Word::SingleQuoted("$X".into())), "$X");
    }

    #[test]
    fn vars_override_env() {
        let mut env = HashMap::new();
        env.insert("X".into(), "env_val".into());
        let mut vars = HashMap::new();
        vars.insert("X".into(), "var_val".into());
        let c = ctx(&env, &vars);
        assert_eq!(c.expand_variables("$X"), "var_val");
    }
}
