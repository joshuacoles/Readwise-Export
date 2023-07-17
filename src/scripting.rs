use rhai::{AST, Dynamic, Engine, Scope};
use rhai::serde::to_dynamic;
use serde_json::json;
use std::cell::RefCell;
use crate::readwise::{Book, Highlight};

pub enum ScriptType {
    Rhai {
        metadata_script: AST,
        engine: Engine,
    },

    Javascript {
        script: RefCell<js_sandbox::Script>
    },
}

impl ScriptType {
    pub fn execute(&self, book: &Book, highlights: &[&Highlight]) -> anyhow::Result<serde_yaml::Value> {
        match self {
            ScriptType::Rhai { metadata_script, engine } => {
                let mut scope = {
                    let mut scope = Scope::new();

                    let book: Dynamic = to_dynamic(book)?;
                    let highlights = to_dynamic(highlights)?;

                    scope.push_dynamic("book", book);
                    scope.push_dynamic("highlights", highlights);

                    scope
                };

                let dynamic: Dynamic = engine.eval_ast_with_scope::<Dynamic>(
                    &mut scope,
                    metadata_script,
                )?;

                Ok(serde_yaml::to_value(&dynamic)?)
            }

            ScriptType::Javascript { script } => {
                let a: serde_json::Value = script.borrow_mut()
                    .call("metadata", &json!({
                        "book": book,
                        "highlights": highlights,
                    }))?;

                Ok(serde_yaml::to_value(&a)?)
            }
        }
    }
}
