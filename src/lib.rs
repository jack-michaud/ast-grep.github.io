mod utils;

use ast_grep_config::{
  deserialize_rule, try_deserialize_matchers, RuleWithConstraint, SerializableMetaVarMatcher,
  SerializableRule,
};
use ast_grep_core::language::Language;
use ast_grep_core::meta_var::MetaVarMatchers;
use ast_grep_core::{Pattern, Node};
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tree_sitter as ts;
use wasm_bindgen::prelude::*;

use std::sync::Mutex;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[derive(Serialize, Deserialize)]
pub struct WASMConfig {
  pub rule: SerializableRule,
  pub fix: Option<String>,
  pub constraints: Option<HashMap<String, SerializableMetaVarMatcher>>,
}

static INSTANCE: Mutex<Option<ts::Language>> = Mutex::new(None);

#[wasm_bindgen(js_name = setupParser)]
pub async fn setup_parser(parser_path: String) -> Result<(), JsError> {
  ts::TreeSitter::init().await?;
  let mut parser = ts::Parser::new()?;
  let lang = get_lang(parser_path).await?;
  parser.set_language(&lang)?;
  let mut curr_lang = INSTANCE.lock().expect_throw("set language error");
  *curr_lang = Some(lang);
  Ok(())
}

#[wasm_bindgen(js_name = findNodes)]
pub fn find_nodes(src: String, config: JsValue) -> Result<JsValue, JsError> {
  let config: WASMConfig = serde_wasm_bindgen::from_value(config)?;
  let lang = INSTANCE
    .lock()
    .expect_throw("get language error")
    .clone()
    .expect_throw("current language is not set");
  let root = lang.ast_grep(src);
  let rule = deserialize_rule(config.rule, lang.clone())?;
  let matchers = if let Some(c) = config.constraints {
    try_deserialize_matchers(c, lang).unwrap()
  } else {
    MetaVarMatchers::default()
  };
  let config = RuleWithConstraint { rule, matchers };
  let ret: Vec<_> = root
    .root()
    .find_all(config)
    .map(|n| {
      let start = n.start_pos();
      let end = n.end_pos();
      vec![start.0, start.1, end.0, end.1]
    })
    .collect();
  let ret = serde_wasm_bindgen::to_value(&ret)?;
  Ok(ret)
}

#[wasm_bindgen(js_name = fixErrors)]
pub fn fix_errors(src: String, config: JsValue) -> Result<String, JsError> {
  let config: WASMConfig = serde_wasm_bindgen::from_value(config)?;
  let lang = INSTANCE
    .lock()
    .expect_throw("get language error")
    .clone()
    .expect_throw("current language is not set");
  let fixer = config.fix.expect_throw("fix is required for rewriting");
  let fixer = Pattern::new(&fixer, lang.clone());
  let root = lang.ast_grep(&src);
  let rule = deserialize_rule(config.rule, lang.clone())?;
  let matchers = if let Some(c) = config.constraints {
    try_deserialize_matchers(c, lang).unwrap()
  } else {
    MetaVarMatchers::default()
  };
  let config = RuleWithConstraint { rule, matchers };
  let edits: Vec<_> = root.root().replace_all(config, fixer);
  let mut new_content = String::new();
  let mut start = 0;
  for edit in edits {
    new_content.push_str(&src[start..edit.position]);
    new_content.push_str(&edit.inserted_text);
    start = edit.position + edit.deleted_length;
  }
  // add trailing statements
  new_content.push_str(&src[start..]);
  Ok(new_content)
}

#[derive(Deserialize, Serialize)]
struct DebugNode {
  kind: String,
  start: (usize, usize),
  end: (usize, usize),
  is_named: bool,
  children: Vec<DebugNode>,
}

fn convert_to_debug_node(n: Node<ts::Language>) -> DebugNode {
  let children = n.children().map(convert_to_debug_node).collect();
  DebugNode {
    kind: n.kind().to_string(),
    start: n.start_pos(),
    end: n.end_pos(),
    is_named: n.is_named(),
    children,
  }
}

#[wasm_bindgen(js_name = dumpASTNodes)]
pub fn dump_ast_nodes(src: String) -> Result<JsValue, JsError> {
  let lang = INSTANCE
    .lock()
    .expect_throw("get language error")
    .clone()
    .expect_throw("current language is not set");
  let root = lang.ast_grep(&src);
  let debug_node = convert_to_debug_node(root.root());
  let ret = serde_wasm_bindgen::to_value(&debug_node)?;
  Ok(ret)
}

#[cfg(target_arch = "wasm32")]
async fn get_lang(parser_path: String) -> Result<ts::Language, JsError> {
  let lang = web_tree_sitter_sg::Language::load_path(&parser_path)
    .await
    .map_err(ts::LanguageError::from)?;
  Ok(ts::Language::from(lang))
}

#[cfg(not(target_arch = "wasm32"))]
async fn get_lang(_path: String) -> Result<ts::Language, JsError> {
  unreachable!()
}
