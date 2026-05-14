use std::collections::HashMap;

pub struct NyAlias {
    alias: HashMap<String, String>
}

impl NyAlias {
    pub fn new() -> Self {
        Self { alias: HashMap::new() }
    }

    pub fn parse_input(&mut self, input: Vec<String>) -> Result<(), String> {
        match input.get(1) {
            Some(part) => {
                match part.as_str() {
                    "--list" => self.list(),

                    "--set" => {
                        match self.insert(input) {
                            Some(err) => return Err(err),
                            None => ()
                        }
                    }

                    _ => return Err("Unknown arg".to_string())
                }
            }
            None => return Err("Missing args! Usage: alias --set <alias> <commands>".to_string())
        }
        
        Ok(())
    }

    pub fn run_alias(&self, input: Vec<String>) -> Option<i32> {
        None
    }

    fn list(&self) {
        for (alias, command) in &self.alias {
            println!("{alias} => {command}")
        }
    }

    fn insert(&mut self, args: Vec<String>) -> Option<String> {
        if args.len() <= 3 {
            return Some("Missing args! Usage: alias --set <alias> <commands>".to_string())
        }

        if let Some(key) =  args.get(2) {
            let cmd = args[3..].join(" ");
            self.alias.insert(key.clone(), cmd);

            return None;
        }

        return Some("Unknown Error! Usage: alias --set <alias> <commands>".to_string())
    }
}