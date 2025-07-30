use rhai::serde::to_dynamic;
use rhai::{Dynamic, Engine, Scope, AST};
use serde_json::json;
use std::cell::RefCell;
use std::path::Path;
use tracing::debug;
use readwise_common::{Book, Highlight};

pub enum ScriptType {
    Rhai {
        metadata_script: AST,
        engine: Engine,
    },

    Javascript {
        script: RefCell<js_sandbox::Script>,
    },
}

impl ScriptType {
    pub fn new(path: &Path) -> anyhow::Result<Self> {
        if path
            .extension()
            .and_then(|e| e.to_str())
            .filter(|e| *e == "js")
            .is_some()
        {
            debug!("Loading javascript metadata script from {:?}", path);
            let script = js_sandbox::Script::from_file(path)?;
            Ok(ScriptType::Javascript {
                script: RefCell::new(script),
            })
        } else {
            debug!("Loading rhai metadata script from {:?}", path);
            let engine = Engine::new();
            let metadata_script = engine.compile_file(path.to_path_buf())?;
            Ok(ScriptType::Rhai {
                metadata_script,
                engine,
            })
        }
    }

    pub fn execute(
        &self,
        book: &Book,
        highlights: &[&Highlight],
    ) -> anyhow::Result<serde_yml::Value> {
        match self {
            ScriptType::Rhai {
                metadata_script,
                engine,
            } => {
                let mut scope = {
                    let mut scope = Scope::new();

                    let book: Dynamic = to_dynamic(book)?;
                    let highlights = to_dynamic(highlights)?;

                    scope.push_dynamic("book", book);
                    scope.push_dynamic("highlights", highlights);

                    scope
                };

                let dynamic: Dynamic =
                    engine.eval_ast_with_scope::<Dynamic>(&mut scope, metadata_script)?;

                Ok(serde_yml::to_value(&dynamic)?)
            }

            ScriptType::Javascript { script } => {
                let a: serde_json::Value = script.borrow_mut().call(
                    "metadata",
                    &json!({
                        "book": book,
                        "highlights": highlights,
                    }),
                )?;

                Ok(serde_yml::to_value(&a)?)
            }
        }
    }
}
