//! The board as tools a model calls, in place of the command lines it writes otherwise.
//!
//! Each tool is one command of the board's language, and a call turns back into that command's line, so the
//! same parser and board apply it and an evaluation compares the two harnesses on the answer's form alone.
//! [`Harness::ToolsChecked`] then shows the model the board its calls drew and lets it put right what it got
//! wrong, a few rounds at most, which is the reason-act-observe loop a tool harness makes possible.

use anyhow::{Context, Result};
use boardcast_board::graph::{parse_ops, Graph, Kind, Op, Ref};
use serde_json::{json, Value};

/// How the board model answers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Harness {
    /// Command lines, held to a grammar on a local model: the app's way.
    #[default]
    Commands,
    /// Tool calls, once per line.
    Tools,
    /// Tool calls, then the board they drew, for the model to correct.
    ToolsChecked,
}

impl Harness {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "commands" => Some(Self::Commands),
            "tools" => Some(Self::Tools),
            "tools-check" => Some(Self::ToolsChecked),
            _ => None,
        }
    }
}

/// How many times the model may correct the board after its first calls.
pub const CHECK_ROUNDS: usize = 2;

const MARKS: [&str; 5] = ["划掉", "圈出", "下划线", "星标", "高亮"];
const ICONS: [&str; 9] = ["数据库", "表格", "手机", "人", "服务器", "文档", "云", "图表", "消息"];

/// Something on the board: an id such as n3 for what is already there, or a name for what this turn makes.
fn target(what: &str) -> Value {
    json!({ "type": "string", "description": what })
}

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({ "type": "function", "function": { "name": name, "description": description, "parameters": { "type": "object", "properties": properties, "required": required } } })
}

/// The tools, in the order the command list gives them.
pub fn definitions() -> Value {
    let thing = "已有的写编号（n3），这一轮刚新建的写名字";
    json!([
        tool("link", "E：连线，新名字会自动建节点", json!({ "from": target(thing), "to": target(thing), "label": target("连线上的动作，可省略") }), &["from", "to"]),
        tool("unlink", "X：删掉一条连线", json!({ "from": target(thing), "to": target(thing) }), &["from", "to"]),
        tool("group", "G：把节点放进同一组", json!({ "name": target("组名"), "members": { "type": "array", "items": target(thing) } }), &["name", "members"]),
        tool("topic", "T：新起一个话题", json!({ "title": target("话题") }), &["title"]),
        tool("item", "L：在话题或条目下面加一条；已有条目换上级也用它", json!({ "parent": target(thing), "text": target("条目") }), &["parent", "text"]),
        tool("question", "Q：记一个还没定的问题，可以挂在某样东西旁边", json!({ "text": target("问题"), "near": target(thing) }), &["text"]),
        tool("formula", "F：记公式或短表达式，可以挂在某样东西旁边", json!({ "text": target("公式"), "near": target(thing) }), &["text"]),
        tool("note", "K：在某样东西旁边写一句补充", json!({ "target": target(thing), "text": target("备注") }), &["target", "text"]),
        tool("mark", "M：标记；[原文] 上可以只标一个词（quote），或一句（quote 加 sentence）", json!({
            "target": target(thing),
            "mark": { "type": "string", "enum": MARKS },
            "quote": target("只标这几个字，可省略"),
            "sentence": { "type": "boolean", "description": "标 quote 所在的整句" }
        }), &["target", "mark"]),
        tool("icon", "I：给节点配图标", json!({ "target": target(thing), "icon": { "type": "string", "enum": ICONS } }), &["target", "icon"]),
        tool("point", "H：只是指给大家看，高亮一下，板上不留字", json!({ "target": target(thing) }), &["target"]),
        tool("rename", "R：改名", json!({ "target": target(thing), "name": target("新名字") }), &["target", "name"]),
        tool("alias", "A：记别名", json!({ "target": target(thing), "alias": target("别名") }), &["target", "alias"]),
        tool("delete", "D：删除（说错了、画错了）", json!({ "target": target(thing) }), &["target"]),
    ])
}

/// 「名字」 for a name, the id as it is for one such as n12.
fn reference(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let id = text.len() > 1 && text.starts_with('n') && text[1..].chars().all(|c| c.is_ascii_digit());
    Some(if id { text.to_string() } else { format!("「{}」", text.replace(['「', '」'], "")) })
}

fn quoted(text: &str) -> Option<String> {
    let text = text.trim().replace(['「', '」'], "");
    (!text.is_empty()).then(|| format!("「{text}」"))
}

/// A call as the command line it stands for, or `None` for one that names no tool or misses what it needs.
pub fn command(name: &str, args: &Value) -> Option<String> {
    let s = |key: &str| args[key].as_str().unwrap_or_default();
    let r = |key: &str| reference(s(key));
    let q = |key: &str| quoted(s(key));
    Some(match name {
        "link" => match q("label") {
            Some(label) => format!("E {} -> {} : {label}", r("from")?, r("to")?),
            None => format!("E {} -> {}", r("from")?, r("to")?),
        },
        "unlink" => format!("X {} -> {}", r("from")?, r("to")?),
        "group" => {
            let members: Vec<String> = args["members"].as_array()?.iter().filter_map(|m| reference(m.as_str()?)).collect();
            format!("G {} {}", q("name")?, members.join(" "))
        }
        "topic" => format!("T {}", q("title")?),
        "item" => format!("L {} {}", r("parent")?, q("text")?),
        "question" | "formula" => {
            let head = if name == "question" { "Q" } else { "F" };
            match r("near") {
                Some(near) => format!("{head} {near} {}", q("text")?),
                None => format!("{head} {}", q("text")?),
            }
        }
        "note" => format!("K {} {}", r("target")?, q("text")?),
        "mark" => {
            let mark = s("mark");
            if !MARKS.contains(&mark) {
                return None;
            }
            match (q("quote"), args["sentence"].as_bool().unwrap_or(false)) {
                (Some(quote), true) => format!("M {} {mark} 句 {quote}", r("target")?),
                (Some(quote), false) => format!("M {} {mark} {quote}", r("target")?),
                (None, _) => format!("M {} {mark}", r("target")?),
            }
        }
        "icon" => {
            let icon = s("icon");
            if !ICONS.contains(&icon) {
                return None;
            }
            format!("I {} {icon}", r("target")?)
        }
        "point" => format!("H {}", r("target")?),
        "rename" => format!("R {} {}", r("target")?, q("name")?),
        "alias" => format!("A {} {}", r("target")?, q("alias")?),
        "delete" => format!("D {}", r("target")?),
        _ => return None,
    })
}

fn arg(r: &Ref) -> String {
    match r {
        Ref::Id(id) => id.clone(),
        Ref::Name(name) => name.clone(),
    }
}

/// A command as the call that stands for it, for showing the fixed examples as tool calls.
pub(crate) fn call(op: &Op) -> Option<(&'static str, Value)> {
    Some(match op {
        Op::E(a, b, label) => ("link", json!({ "from": arg(a), "to": arg(b), "label": label })),
        Op::X(a, b) => ("unlink", json!({ "from": arg(a), "to": arg(b) })),
        Op::G(name, members) => ("group", json!({ "name": name, "members": members.iter().map(arg).collect::<Vec<_>>() })),
        Op::T(title) => ("topic", json!({ "title": title })),
        Op::L(parent, text) => ("item", json!({ "parent": arg(parent), "text": text })),
        Op::Attached(kind, near, text) => (if *kind == Kind::Question { "question" } else { "formula" }, json!({ "text": text, "near": near.as_ref().map(arg) })),
        Op::K(target, text) => ("note", json!({ "target": arg(target), "text": text })),
        Op::M(target, mark, quote) => ("mark", json!({ "target": arg(target), "mark": mark, "quote": quote.as_ref().map(|q| &q.0), "sentence": quote.as_ref().is_some_and(|q| q.1) })),
        Op::I(target, icon) => ("icon", json!({ "target": arg(target), "icon": icon })),
        Op::H(target) => ("point", json!({ "target": arg(target) })),
        Op::R(target, name) => ("rename", json!({ "target": arg(target), "name": name })),
        Op::A(target, alias) => ("alias", json!({ "target": arg(target), "alias": alias })),
        Op::D(target) => ("delete", json!({ "target": arg(target) })),
        Op::N(_) => return None,
    })
}

/// The system prompt with the command list turned into the tools' terms; the rules keep their letters, which
/// the legend names.
fn system(text: &str) -> String {
    let start = text.find("只输出操作").unwrap_or(text.len());
    let end = text.find("规则：").unwrap_or(text.len());
    let tools = "用工具改白板：一次调用做一件事，这一句需要的改动一次都调用完。东西要用编号或名字指明：已有的写编号（n3），这一轮刚新建的还没有编号，写名字，新名字会自动建节点。这句不需要改白板（寒暄、跑题、复述），就不调用任何工具，只回复 -。\n规则里的字母对应工具：E=link，X=unlink，G=group，T=topic，L=item，Q=question，F=formula，K=note，M=mark，I=icon，H=point，R=rename，A=alias，D=delete。\n\n";
    format!("{}{tools}{}", &text[..start], &text[end..])
}

/// The prompt's messages in tool form: the system prompt's command list given as tools, and each example's
/// reply as the calls it stands for, each answered.
pub fn messages(commands: &[Value]) -> Vec<Value> {
    let mut out = vec![];
    let mut calls = 0;
    for m in commands {
        match m["role"].as_str() {
            Some("system") => out.push(json!({ "role": "system", "content": system(m["content"].as_str().unwrap_or_default()) })),
            Some("assistant") => {
                let text = m["content"].as_str().unwrap_or_default();
                let made: Vec<(&str, Value)> = parse_ops(text).iter().filter_map(call).collect();
                if made.is_empty() {
                    out.push(json!({ "role": "assistant", "content": "-" }));
                    continue;
                }
                let ids: Vec<String> = made.iter().map(|_| {
                    calls += 1;
                    format!("example-{calls}")
                }).collect();
                let tool_calls: Vec<Value> = made.iter().zip(&ids).map(|((name, args), id)| json!({ "id": id, "type": "function", "function": { "name": name, "arguments": args.to_string() } })).collect();
                out.push(json!({ "role": "assistant", "content": "", "tool_calls": tool_calls }));
                out.extend(ids.iter().map(|id| json!({ "role": "tool", "tool_call_id": id, "content": "ok" })));
            }
            _ => out.push(m.clone()),
        }
    }
    out
}

/// One round's calls from a reply: each call's id and the command it stands for, if it is one.
pub fn calls(reply: &Value) -> Vec<(String, Option<String>)> {
    let message = &reply["choices"][0]["message"];
    message["tool_calls"]
        .as_array()
        .map(|calls| {
            calls
                .iter()
                .map(|c| {
                    let name = c["function"]["name"].as_str().unwrap_or_default();
                    let args: Value = c["function"]["arguments"].as_str().and_then(|a| serde_json::from_str(a).ok()).unwrap_or(Value::Null);
                    (c["id"].as_str().unwrap_or_default().to_string(), command(name, &args))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// What the model is told after its calls, round by round: whether each call was used, and the board then.
pub fn results(calls: &[(String, Option<String>)], board: &Graph, last: bool) -> Vec<Value> {
    let n = calls.len();
    calls
        .iter()
        .enumerate()
        .map(|(i, (id, command))| {
            let mut text = match command {
                Some(c) => format!("ok：{c}"),
                None => "没用上：工具名或参数不对".to_string(),
            };
            if i + 1 == n {
                text.push_str(&format!("\n白板现在：\n{}", board.describe()));
                text.push_str(if last { "\n这一句到此为止。" } else { "\n对照「新的一句」检查：画错的（多连的线、名字不对、放错地方）用工具改掉，漏掉的补上。都对就不调用工具，只回复 -。" });
            }
            json!({ "role": "tool", "tool_call_id": id, "content": text })
        })
        .collect()
}

/// The assistant's turn as it goes back into the conversation, with what a reasoning model thought, which it
/// must be given back while a turn with tools goes on.
pub fn echo(reply: &Value) -> Result<Value> {
    let message = reply["choices"][0]["message"].clone();
    message.as_object().context("the model's reply has no message")?;
    let mut back = json!({ "role": "assistant", "content": message["content"].as_str().unwrap_or_default() });
    if let Some(calls) = message.get("tool_calls").filter(|c| !c.is_null()) {
        back["tool_calls"] = calls.clone();
    }
    if let Some(thought) = message.get("reasoning_content").filter(|c| !c.is_null()) {
        back["reasoning_content"] = thought.clone();
    }
    Ok(back)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_is_the_command_it_stands_for() {
        assert_eq!(command("link", &json!({ "from": "n1", "to": "网关", "label": "转发" })).unwrap(), "E n1 -> 「网关」 : 「转发」");
        assert_eq!(command("link", &json!({ "from": "用户", "to": "n2" })).unwrap(), "E 「用户」 -> n2");
        assert_eq!(command("mark", &json!({ "target": "n3", "mark": "圈出", "quote": "predicting", "sentence": true })).unwrap(), "M n3 圈出 句 「predicting」");
        assert_eq!(command("question", &json!({ "text": "要不要上 MQ" })).unwrap(), "Q 「要不要上 MQ」");
        assert_eq!(command("group", &json!({ "name": "后端", "members": ["n1", "支付服务"] })).unwrap(), "G 「后端」 n1 「支付服务」");
        assert!(command("mark", &json!({ "target": "n3", "mark": "boldface" })).is_none(), "only the board's marks");
        assert!(command("teleport", &json!({})).is_none());
        assert!(command("link", &json!({ "from": "n1" })).is_none(), "a link needs both ends");
    }

    #[test]
    fn every_example_reply_turns_into_calls_and_back() {
        let prompts: Value = serde_json::from_str(include_str!("../prompts.json")).unwrap();
        for lang in ["zh", "en"] {
            for m in prompts[lang].as_array().unwrap().iter().filter(|m| m["role"] == "assistant") {
                let text = m["content"].as_str().unwrap();
                let ops = parse_ops(text);
                let back: Vec<String> = ops.iter().filter_map(call).filter_map(|(name, args)| command(name, &args)).collect();
                assert_eq!(parse_ops(&back.join("\n")), ops, "{text}");
            }
        }
    }

    #[test]
    fn the_tools_prompt_keeps_the_rules_and_drops_the_command_list() {
        let prompts: Value = serde_json::from_str(include_str!("../prompts.json")).unwrap();
        let tools = messages(prompts["zh"].as_array().unwrap());
        let system = tools[0]["content"].as_str().unwrap();
        assert!(system.contains("规则：") && system.contains("E=link") && !system.contains("只输出操作"), "{system}");
        assert!(tools.iter().any(|m| m["tool_calls"].is_array()), "examples are calls");
        // Every call in the examples is answered, as a conversation with tools must be.
        let asked: usize = tools.iter().filter_map(|m| m["tool_calls"].as_array()).map(Vec::len).sum();
        assert_eq!(asked, tools.iter().filter(|m| m["role"] == "tool").count());
    }
}
