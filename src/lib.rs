//! Convert between XML nodes ([treexml](https://github.com/rahulg/treexml-rs)) and JSON objects ([serde-json](https://github.com/serde-rs/json)).
//!
//! ## Example
//! ```
//! extern crate treexml;
//!
//! #[macro_use]
//! extern crate serde_json;
//!
//! extern crate node2object;
//!
//! fn main() {
//!     let dom_root = treexml::Document::parse("
//!         <population>
//!           <entry>
//!             <name>Alex</name>
//!             <height>173.5</height>
//!           </entry>
//!           <entry>
//!             <name>Mel</name>
//!             <height>180.4</height>
//!           </entry>
//!         </population>
//!     ".as_bytes()).unwrap().root.unwrap();
//!
//!     assert_eq!(serde_json::Value::Object(node2object::node2object(&dom_root)), json!(
//!         {
//!           "population": {
//!             "entry": [
//!               { "name": "Alex", "height": 173.5 },
//!               { "name": "Mel", "height": 180.4 }
//!             ]
//!           }
//!         }
//!     ));
//! }
//! ```

#![feature(test)]

extern crate treexml;
extern crate inflector;

#[macro_use]
extern crate serde_json;
extern crate test;

use serde_json::{Map, Number, Value};
use inflector::cases::snakecase::to_snake_case;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum XMLNodeType {
    Empty,
    Text,
    Attributes,
    TextAndAttributes,
    Parent,
    SemiStructured,
}

fn scan_xml_node(e: &treexml::Element) -> XMLNodeType {
    if e.children.is_empty() {
        if e.text.is_none() && e.cdata.is_none() {
            if e.attributes.is_empty() {
                XMLNodeType::Empty
            } else {
                XMLNodeType::Attributes
            }
        } else {
            if e.attributes.is_empty() {
                XMLNodeType::Text
            } else {
                XMLNodeType::TextAndAttributes
            }
        }
    } else {
        if e.text.is_some() || e.cdata.is_some() {
            XMLNodeType::SemiStructured
        } else {
            XMLNodeType::Parent
        }
    }
}

fn parse_text(text: &str) -> Value {
    match text.parse::<f64>() {
        Ok(v) => match Number::from_f64(v) {
            Some(v) => {
                return Value::Number(v);
            }
            _ => {}
        },
        _ => {}
    }

    match text.parse::<bool>() {
        Ok(v) => {
            return Value::Bool(v);
        }
        _ => {}
    }

    Value::String(text.into())
}

fn parse_text_contents(e: &treexml::Element) -> Value {
    let text = format!(
        "{}{}",
        &e.text.clone().unwrap_or(String::new()),
        &e.cdata.clone().unwrap_or(String::new())
    );
    parse_text(&text)
}

fn convert_node_aux(e: &treexml::Element) -> Option<Value> {
    match scan_xml_node(e) {
        XMLNodeType::Parent => {
            let mut data = Map::new();
            let mut vectorized = std::collections::HashSet::new();

            if e.attributes.len() > 0 {
                for (k, v) in e.attributes.clone().into_iter() {
                    data.insert(to_snake_case(&k), parse_text(&v));
                }
            }
            for c in &e.children {
                match convert_node_aux(c) {
                    Some(v) => {
                        let snake_cased_name = to_snake_case(&c.name);
                        use std::str::FromStr;
                        let snake_cased_name = if snake_cased_name.eq("option") {
                            "option_tag".to_string()
                        } else {
                            snake_cased_name
                        };
                            if !vectorized.contains(&snake_cased_name) {
                                data.insert(snake_cased_name.clone(), Value::Array(vec![v]));
                                vectorized.insert(snake_cased_name);
                            } else {
                                data.get_mut(&snake_cased_name)
                                    .unwrap()
                                    .as_array_mut()
                                    .unwrap()
                                    .push(v);
                            }
                    }
                    _ => {}
                }
            }
            Some(Value::Object(data))
        }
        XMLNodeType::Text => Some(parse_text_contents(e)),
        XMLNodeType::Attributes => Some(Value::Object(
            e.attributes
                .clone()
                .into_iter()
                .map(|(k, v)| (to_snake_case(&k), parse_text(&v)))
                .collect(),
        )),
        XMLNodeType::TextAndAttributes => Some(Value::Object(
            e.attributes
                .clone()
                .into_iter()
                .map(|(k, v)| (to_snake_case(&k), parse_text(&v)))
                .chain(vec![("text".to_string(), parse_text_contents(&e))])
                .collect(),
        )),
        _ => None,
    }
}

/// Converts treexml::Element into a serde_json hashmap. The latter can be wrapped in Value::Object.
pub fn node2object(e: &treexml::Element) -> Map<String, Value> {
    let mut data = Map::new();
    data.insert(to_snake_case(&e.name), convert_node_aux(e).unwrap_or(Value::Null));
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    use test::Bencher;

    #[bench]
    fn bench(b: &mut Bencher) {
        let xml = include_str!("../examples/xml.xml");
        b.iter(|| {
            let n = test::black_box(10);
            (0..n).for_each(|_| {
                let xml = treexml::Document::parse(xml.as_bytes()).unwrap();
                let xml = xml.root.unwrap();
                let _ = node2object(&xml);
            })
        })
    }

    #[test]
    fn smart_list_detection() {
        let raw_xml = treexml::Document::parse(r#"<a>
            <b first="1"/>
            <b first="2"/>
            <c first="3"/>
            </a>
        "#.as_bytes()).unwrap().root.unwrap();
        let actual = Value::Object(node2object(&raw_xml));
        assert_eq!(actual, json!({
            "a": {
                "b": [ { "first": 1.0 }, { "first": 2.0 } ],
                "c": [ { "first": 3.0 } ]
            }
        }));
    }

    #[test]
    fn node2object_empty() {
        let fixture = treexml::Element::new("e");
        let scan_result = XMLNodeType::Empty;
        let conv_result = json!({ "e": null });

        assert_eq!(scan_result, scan_xml_node(&fixture));
        assert_eq!(conv_result, Value::Object(node2object(&fixture)));
    }

    #[test]
    fn node2object_text() {
        let mut fixture = treexml::Element::new("player");
        fixture.text = Some("Kolya".into());
        let scan_result = XMLNodeType::Text;
        let conv_result = json!({"player": "Kolya"});

        assert_eq!(scan_result, scan_xml_node(&fixture));
        assert_eq!(conv_result, Value::Object(node2object(&fixture)));
    }

    #[test]
    fn node2object_attributes() {
        let mut fixture = treexml::Element::new("player");
        fixture.attributes.insert("score".into(), "9000".into());
        let scan_result = XMLNodeType::Attributes;
        let conv_result = json!({ "player": json!({"@score": 9000.0}) });

        assert_eq!(scan_result, scan_xml_node(&fixture));
        assert_eq!(conv_result, Value::Object(node2object(&fixture)));
    }

    #[test]
    fn node2object_text_and_attributes() {
        let mut fixture = treexml::Element::new("player");
        fixture.text = Some("Kolya".into());
        fixture.attributes.insert("score".into(), "9000".into());
        let scan_result = XMLNodeType::TextAndAttributes;
        let conv_result = json!({ "player": json!({"#text": "Kolya", "@score": 9000.0}) });

        assert_eq!(scan_result, scan_xml_node(&fixture));
        assert_eq!(conv_result, Value::Object(node2object(&fixture)));
    }

    #[test]
    fn node2object_parent() {
        let mut fixture = treexml::Element::new("ServerData");
        fixture.children = vec![
            {
                let mut node = treexml::Element::new("Player");
                node.text = Some("Kolya".into());
                node
            },
            {
                let mut node = treexml::Element::new("Player");
                node.text = Some("Petya".into());
                node
            },
            {
                let mut node = treexml::Element::new("Player");
                node.text = Some("Misha".into());
                node
            },
        ];
        let scan_result = XMLNodeType::Parent;
        let conv_result =
            json!({ "ServerData": json!({ "Player": [ "Kolya", "Petya", "Misha" ] }) });

        assert_eq!(scan_result, scan_xml_node(&fixture));
        assert_eq!(conv_result, Value::Object(node2object(&fixture)));
    }

    #[test]
    fn node2object_preserve_attributes_parents() {
        let dom_root = treexml::Document::parse(
            "
        <a pizza=\"hotdog\">           
          <b frenchfry=\"milkshake\">
            <c>scotch</c>
          </b>
        </a>
    "
                .as_bytes(),
        ).unwrap()
            .root
            .unwrap();

        let json_result = Value::Object(node2object(&dom_root));
        let expected = json!({
            "a": json!({
                "@pizza": "hotdog",
                "b": json!({
                    "@frenchfry": "milkshake",
                    "c":  "scotch"
                })
            })
        });
        assert_eq!(json_result, expected);
    }
}
