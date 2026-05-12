use std::collections::HashMap;

pub struct Vars {
    vars: HashMap<String, String>,
}

impl Default for Vars {
    fn default() -> Self {
        Self::new()
    }
}

impl Vars {
    pub fn new() -> Vars {
        Vars {
            vars: HashMap::new(),
        }
    }

    pub fn expand_vars(&self, s: &str) -> String {
        let mut out = String::new();
        let chars: Vec<char> = s.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            if chars[i] != '$' {
                out.push(chars[i]);
                i += 1;
                continue;
            }

            // ${VAR}
            if i + 1 < chars.len() && chars[i + 1] == '{' {
                let mut j = i + 2;

                while j < chars.len() && chars[j] != '}' {
                    j += 1;
                }

                if j < chars.len() {
                    let name: String = chars[i + 2..j].iter().collect();
                    out.push_str(&get_var(&name, &self.vars));
                    i = j + 1;
                    continue;
                }
            }

            // $VAR
            let mut j = i + 1;

            while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                j += 1;
            }

            if j == i + 1 {
                out.push('$');
                i += 1;
                continue;
            }

            let name: String = chars[i + 1..j].iter().collect();
            out.push_str(&get_var(&name, &self.vars));
            i = j;
        }

        out
    }

    pub fn print_vars(&self) {
        for (k, v) in &self.vars {
            println!("${} = {}", k, v)
        }
    }

    pub fn insert(&mut self, line: &str) -> Result<i32, String> {
        if let Some((key, value)) = line.split_once("=") {
            let key = key.strip_prefix('$').unwrap_or(key);

            self.vars.insert(key.to_string(), value.to_string());

            return Ok(0);
        }

        Err("An unexpected Error Accoured".to_string())
    }

    pub fn remove(&mut self, key: &str) -> Result<i32, String> {
        let var = key.strip_prefix("$").unwrap_or(key);

        if self.vars.remove(var).is_some() {
            Ok(0)
        } else {
            Err(format!("Variable '{var}' not found"))
        }
    }

    pub fn get_vars(&self) -> HashMap<String, String> {
        self.vars.clone()
    }
}

fn get_var(name: &str, env_vars: &HashMap<String, String>) -> String {
    env_vars
        .get(name)
        .cloned()
        .or_else(|| std::env::var(name).ok())
        .unwrap_or_default()
}
