//! The words the server prints, in English, with Chinese as a translation.
//!
//! English is the primitive. A message is written as an English template with `{}` holes, which
//! are filled in order. Chinese output looks the template up in [`ZH`] and fills the translation the
//! same way, so both languages carry the same values. A template without a translation prints in
//! English, so a missing entry shows up as English rather than as nothing.
//!
//! The studio's interface strings are translated in the front end (`ui/src/i18n.ts`). This table
//! covers the text the server produces: the ladder in `next.rs` and the words its command line
//! prints.

/// The language a message is printed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    /// English, the primitive text. The default.
    #[default]
    En,
    /// Chinese, from the translation table.
    Zh,
}

impl Lang {
    /// The language named by a code such as `en` or `zh`, or `None` for any other code.
    pub fn parse(code: &str) -> Option<Lang> {
        match code {
            "en" => Some(Lang::En),
            "zh" => Some(Lang::Zh),
            _ => None,
        }
    }
}

/// Translate `template` into `lang` and fill its holes with `args`, in order.
pub fn tr<S: AsRef<str>>(lang: Lang, template: &str, args: &[S]) -> String {
    let text = match lang {
        Lang::En => template,
        Lang::Zh => zh(template).unwrap_or(template),
    };
    fill(text, args)
}

/// A template with no holes, translated into `lang`.
pub fn text(lang: Lang, template: &str) -> String {
    tr(lang, template, &[] as &[&str])
}

/// The singular or the plural template, by the count `n`.
pub fn plural(n: usize, one: &'static str, many: &'static str) -> &'static str {
    if n == 1 {
        one
    } else {
        many
    }
}

fn zh(template: &str) -> Option<&'static str> {
    ZH.iter()
        .find(|(english, _)| *english == template)
        .map(|(_, chinese)| *chinese)
}

fn fill<S: AsRef<str>>(template: &str, args: &[S]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    let mut args = args.iter();
    while let Some(at) = rest.find("{}") {
        out.push_str(&rest[..at]);
        match args.next() {
            Some(arg) => out.push_str(arg.as_ref()),
            None => out.push_str("{}"),
        }
        rest = &rest[at + 2..];
    }
    out.push_str(rest);
    out
}

/// English template, Chinese translation. Both carry the same holes in the same order.
const ZH: &[(&str, &str)] = &[
    // Stage labels, and the labels of the rows that `render` prints.
    ("1/6 requirements", "1/6 需求"),
    ("2/6 questions", "2/6 分岔点"),
    ("3/6 allocation", "3/6 分配"),
    ("4/6 values", "4/6 参数"),
    ("5/6 interfaces", "5/6 接口"),
    ("6/6 verification", "6/6 验证"),
    ("complete", "完成"),
    ("stage", "阶段"),
    ("done", "已有"),
    ("waiting", "等待"),
    ("next", "下一步"),
    (" and {} more", " 等另外 {} 项"),
    // The ladder's messages, in the order the stages check them.
    ("the model reads and checks clean", "模型能读，检查通过"),
    (
        "the model has no requirement: with no constraint, nothing downstream has anything to check against",
        "模型里没有 requirement：没有约束，后面的结构就没有验收对象",
    ),
    ("{} requirement", "{} 条requirement"),
    ("{} requirements", "{} 条requirement"),
    ("{} requirement, all constraints decidable", "{} 条requirement，约束都可判定"),
    ("{} requirements, all constraints decidable", "{} 条requirement，约束都可判定"),
    (
        "{} has a constraint made only of literals (`{1 > 0}`), so it constrains nothing",
        "{} 的约束只由字面量构成（`{1 > 0}`），它不约束任何东西",
    ),
    (
        "{} open question nobody has answered; the first is {}",
        "{} 条问题还没有人回答，其中最前面的是 {}",
    ),
    (
        "{} open questions nobody has answered; the first is {}",
        "{} 条问题还没有人回答，其中最前面的是 {}",
    ),
    ("it blocks: {}", "它阻塞：{}"),
    (
        "sysml answer {} --id {} --text \"...\" --by \"human:<name>\"",
        "sysml answer {} --id {} --text \"...\" --by \"human:<姓名>\"",
    ),
    (
        "{} concern, what a stakeholder cares about",
        "{} 条 concern（利益相关方关心的事）",
    ),
    (
        "{} concerns, what stakeholders care about",
        "{} 条 concern（利益相关方关心的事）",
    ),
    (
        "no requirement carries {}: the model names these concerns, but no requirement is derived from them",
        "{} 没有 requirement 承接：模型写了这些 concern，但没有需求是从它们派生的",
    ),
    (
        "write the derivation with a library connection: `connection d : Derivation connect <concern> to <requirement>;`",
        "派生用标准库的连接写：`connection d : Derivation connect <concern> to <requirement>;`",
    ),
    ("requirements complete, no open questions", "需求齐备，没有未决问题"),
    (
        "no subject or satisfy link reaches {}: the requirement hangs free, and no structure carries it",
        "{} 既没有 subject 也没有 satisfy 指向：需求悬空，没有结构承担它",
    ),
    ("every requirement has structure that carries it", "每条需求都有承担它的结构"),
    (
        "{} value has no ledger entry (the first is {}); every number in the model needs a source",
        "{} 个值没有台账记录（最先一个是 {}）；模型里的每个数字都要有来源",
    ),
    (
        "{} values have no ledger entry (the first is {}); every number in the model needs a source",
        "{} 个值没有台账记录（最先一个是 {}）；模型里的每个数字都要有来源",
    ),
    ("every value has a source record", "每个值都有来源记录"),
    (
        "{} assumption nobody has approved ({}): an assumption needs a person to accept the risk",
        "{} 条假定没有人批准过（{}）：假定要有人接受风险",
    ),
    (
        "{} assumptions nobody has approved ({}): an assumption needs a person to accept the risk",
        "{} 条假定没有人批准过（{}）：假定要有人接受风险",
    ),
    (
        "sysml approve {} --id {} --by \"human:<name>\"",
        "sysml approve {} --id {} --by \"human:<姓名>\"",
    ),
    ("values and sources complete", "值与来源齐备"),
    (
        "{} connection has an end that does not exist: the interface names one end, and the other is missing",
        "{} 条连接的端点不存在：接口说了一头，另一头没有",
    ),
    (
        "{} connections have an end that does not exist: the interface names one end, and the other is missing",
        "{} 条连接的端点不存在：接口说了一头，另一头没有",
    ),
    ("structure, values and interfaces complete", "结构、参数、接口齐备"),
    (
        "no verdict yet for {}: a value or unit is missing, so the result is neither pass nor fail",
        "{} 还没有判定结果（缺值或缺单位，不是通过也不是不通过）",
    ),
    ("all six gates pass", "六道关都过了"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holes_are_filled_in_order() {
        assert_eq!(tr(Lang::En, "{} of {}", &["2", "5"]), "2 of 5");
    }

    #[test]
    fn a_template_without_a_translation_prints_in_english() {
        assert_eq!(tr(Lang::Zh, "untranslated {}", &["x"]), "untranslated x");
    }

    #[test]
    fn a_translation_keeps_the_holes_of_its_template() {
        for (english, chinese) in ZH {
            assert_eq!(
                english.matches("{}").count(),
                chinese.matches("{}").count(),
                "the holes differ: {english}"
            );
        }
    }

    #[test]
    fn each_template_has_one_translation() {
        let mut seen = std::collections::HashSet::new();
        for (english, _) in ZH {
            assert!(seen.insert(*english), "two translations for {english}");
        }
    }
}
