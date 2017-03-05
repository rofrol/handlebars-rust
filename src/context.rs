use serde_json::value::{Value as Json, ToJson, Map};

use pest::prelude::*;
use std::collections::{VecDeque, BTreeMap};

use grammar::{Rdp, Rule};

static DEFAULT_VALUE: Json = Json::Null;

pub type Object = BTreeMap<String, Json>;

/// The context wrap data you render on your templates.
///
#[derive(Debug, Clone)]
pub struct Context {
    data: Json,
}

#[inline]
fn parse_json_visitor_inner<'a>(path_stack: &mut VecDeque<&'a str>, path: &'a str) {
    let path_in = StringInput::new(path);
    let mut parser = Rdp::new(path_in);

    if parser.path() {
        for seg in parser.queue().iter() {
            match seg.rule {
                Rule::path_var | Rule::path_idx | Rule::path_key => {}
                Rule::path_up => {
                    path_stack.pop_back();
                }
                Rule::path_id | Rule::path_raw_id | Rule::path_num_id => {
                    let id = &path[seg.start..seg.end];
                    path_stack.push_back(id);
                }
                _ => {}
            }
        }
    }
}

#[inline]
fn parse_json_visitor<'a>(path_stack: &mut VecDeque<&'a str>,
                          base_path: &'a str,
                          path_context: &'a VecDeque<String>,
                          relative_path: &'a str) {
    let path_in = StringInput::new(relative_path);
    let mut parser = Rdp::new(path_in);

    if parser.path() {
        let mut path_context_depth: i64 = -1;

        let mut iter = parser.queue().iter();
        loop {
            if let Some(sg) = iter.next() {
                if sg.rule == Rule::path_up {
                    path_context_depth += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        if path_context_depth >= 0 {
            if let Some(context_base_path) = path_context.get(path_context_depth as usize) {
                parse_json_visitor_inner(path_stack, context_base_path);
            } else {
                parse_json_visitor_inner(path_stack, base_path);
            }
        } else {
            parse_json_visitor_inner(path_stack, base_path);
        }

        parse_json_visitor_inner(path_stack, relative_path);
    }
    // TODO: report invalid path
}

fn merge_json(base: &Json, addition: &Object) -> Json {
    let mut base_map = match base {
        &Json::Object(ref m) => m.clone(),
        _ => {
            let mut map = Map::new();
            map.insert("this".to_owned(), base.clone());
            map
        }
    };

    for (k, v) in addition.iter() {
        base_map.insert(k.clone(), v.clone());
    }

    Json::Object(base_map)
}

impl Context {
    /// Create a context with null data
    pub fn null() -> Context {
        Context { data: Json::Null }
    }

    /// Create a context with given data
    pub fn wraps<T: ToJson>(e: &T) -> Context {
        Context { data: to_json(e) }
    }

    /// Extend current context with another JSON object
    /// If current context is a JSON object, it's identical to a normal merge
    /// Otherwise, the current value will be stored in new JSON object with key `this`, and merged
    /// keys are also available.
    pub fn extend(&self, hash: &Object) -> Context {
        let new_data = merge_json(&self.data, hash);
        Context { data: new_data }
    }

    /// Navigate the context with base path and relative path
    /// Typically you will set base path to `RenderContext.get_path()`
    /// and set relative path to helper argument or so.
    ///
    /// If you want to navigate from top level, set the base path to `"."`
    pub fn navigate(&self,
                    base_path: &str,
                    path_context: &VecDeque<String>,
                    relative_path: &str)
                    -> &Json {
        let mut path_stack: VecDeque<&str> = VecDeque::new();
        parse_json_visitor(&mut path_stack, base_path, path_context, relative_path);

        let paths: Vec<&str> = path_stack.iter().map(|x| *x).collect();
        let mut data: &Json = &self.data;
        for p in paths.iter() {
            if *p == "this" && data.as_object().and_then(|m| m.get("this")).is_none() {
                continue;
            }
            data = match *data {
                Json::Array(ref l) => {
                    p.parse::<usize>()
                     .and_then(|idx_u| Ok(l.get(idx_u).unwrap_or(&DEFAULT_VALUE)))
                     .unwrap_or(&DEFAULT_VALUE)
                }
                Json::Object(ref m) => m.get(*p).unwrap_or(&DEFAULT_VALUE),
                _ => &DEFAULT_VALUE,
            }
        }
        data
    }

    pub fn data(&self) -> &Json {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut Json {
        &mut self.data
    }
}

/// Render Json data with default format
pub trait JsonRender {
    fn render(&self) -> String;
}

pub trait JsonTruthy {
    fn is_truthy(&self) -> bool;
}

impl JsonRender for Json {
    fn render(&self) -> String {
        match *self {
            Json::String(ref s) => s.to_string(),
            Json::Bool(i) => i.to_string(),
            Json::Number(ref n) => n.to_string(),
            Json::Null => "".to_owned(),
            Json::Array(ref a) => {
                let mut buf = String::new();
                buf.push('[');
                for i in a.iter() {
                    buf.push_str(i.render().as_ref());
                    buf.push_str(", ");
                }
                buf.push(']');
                buf
            }
            Json::Object(_) => "[object]".to_owned(),
        }
    }
}

pub fn to_json<T>(src: &T) -> Json
    where T: ToJson
{
    src.to_json().unwrap_or(Json::Null)
}

pub fn as_string(src: &Json) -> Option<&str> {
    src.as_str()
}

impl JsonTruthy for Json {
    fn is_truthy(&self) -> bool {
        match *self {
            Json::Bool(ref i) => *i,
            Json::Number(ref n) => n.as_f64().map(|f| f.is_normal()).unwrap_or(false),
            Json::Null => false,
            Json::String(ref i) => i.len() > 0,
            Json::Array(ref i) => i.len() > 0,
            Json::Object(ref i) => i.len() > 0,
        }
    }
}

#[cfg(test)]
mod test {
    use context::{self, JsonRender, Context};
    use std::collections::{VecDeque, BTreeMap};
    use serde_json::value::{Value as Json, Map};

    #[test]
    fn test_json_render() {
        let raw = "<p>Hello world</p>\n<p thing=\"hello\"</p>";
        let thing = Json::String(raw.to_string());

        assert_eq!(raw, thing.render());
    }

    #[derive(Serialize)]
    struct Address {
        city: String,
        country: String,
    }

    #[derive(Serialize)]
    struct Person {
        name: String,
        age: i16,
        addr: Address,
        titles: Vec<String>,
    }

    #[test]
    fn test_render() {
        let v = "hello";
        let ctx = Context::wraps(&v.to_string());
        assert_eq!(ctx.navigate(".", &VecDeque::new(), "this").render(),
                   v.to_string());
    }

    #[test]
    fn test_navigation() {
        let addr = Address {
            city: "Beijing".to_string(),
            country: "China".to_string(),
        };

        let person = Person {
            name: "Ning Sun".to_string(),
            age: 27,
            addr: addr,
            titles: vec!["programmer".to_string(), "cartographier".to_string()],
        };

        let ctx = Context::wraps(&person);
        assert_eq!(ctx.navigate(".", &VecDeque::new(), "./name/../addr/country").render(),
                   "China".to_string());
        assert_eq!(ctx.navigate(".", &VecDeque::new(), "addr.[country]").render(),
                   "China".to_string());
        assert_eq!(ctx.navigate(".", &VecDeque::new(), "addr.[\"country\"]").render(),
                   "China".to_string());
        assert_eq!(ctx.navigate(".", &VecDeque::new(), "addr.['country']").render(),
                   "China".to_string());

        let v = true;
        let ctx2 = Context::wraps(&v);
        assert_eq!(ctx2.navigate(".", &VecDeque::new(), "this").render(),
                   "true".to_string());

        assert_eq!(ctx.navigate(".", &VecDeque::new(), "titles[0]").render(),
                   "programmer".to_string());
        assert_eq!(ctx.navigate(".", &VecDeque::new(), "titles.[0]").render(),
                   "programmer".to_string());

        assert_eq!(ctx.navigate(".", &VecDeque::new(), "titles[0]/../../age").render(),
                   "27".to_string());
        assert_eq!(ctx.navigate(".", &VecDeque::new(), "this.titles[0]/../../age").render(),
                   "27".to_string());

    }

    #[test]
    fn test_this() {
        let mut map_with_this = Map::new();
        map_with_this.insert("this".to_string(), context::to_json(&"hello"));
        map_with_this.insert("age".to_string(), context::to_json(&5usize));
        let ctx1 = Context::wraps(&map_with_this);

        let mut map_without_this = Map::new();
        map_without_this.insert("age".to_string(), context::to_json(&4usize));
        let ctx2 = Context::wraps(&map_without_this);

        assert_eq!(ctx1.navigate(".", &VecDeque::new(), "this").render(),
                   "hello".to_owned());
        assert_eq!(ctx2.navigate(".", &VecDeque::new(), "age").render(),
                   "4".to_owned());
    }

    #[test]
    fn test_extend() {
        let mut map = Map::new();
        map.insert("age".to_string(), context::to_json(&4usize));
        let ctx1 = Context::wraps(&map);

        let s = "hello".to_owned();
        let ctx2 = Context::wraps(&s);

        let mut hash = BTreeMap::new();
        hash.insert("tag".to_owned(), context::to_json(&"h1"));

        let ctx_a1 = ctx1.extend(&hash);
        assert_eq!(ctx_a1.navigate(".", &VecDeque::new(), "age").render(),
                   "4".to_owned());
        assert_eq!(ctx_a1.navigate(".", &VecDeque::new(), "tag").render(),
                   "h1".to_owned());

        let ctx_a2 = ctx2.extend(&hash);
        assert_eq!(ctx_a2.navigate(".", &VecDeque::new(), "this").render(),
                   "hello".to_owned());
        assert_eq!(ctx_a2.navigate(".", &VecDeque::new(), "tag").render(),
                   "h1".to_owned());
    }

    #[test]
    fn test_key_name_with_this() {
        let m = btreemap!{
            "this_name".to_string() => "the_value".to_string()
        };
        let ctx = Context::wraps(&m);
        assert_eq!(ctx.navigate(".", &VecDeque::new(), "this_name").render(),
                   "the_value".to_string());
    }
}
