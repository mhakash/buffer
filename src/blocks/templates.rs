//! Starting points for a new block type (the Blocks page's + menu). Scripts live beside this file.

use std::collections::BTreeMap;

use super::model::{BlockType, Param, ParamKind};

pub struct Template {
    pub label: &'static str,
    /// Suggested folder name (made unique by the page).
    pub name: &'static str,
    pub ty: BlockType,
    pub script: &'static str,
}

fn param(name: &str, kind: ParamKind, default: &str) -> Param {
    Param {
        name: name.into(),
        kind,
        default: default.into(),
    }
}

pub fn all() -> Vec<Template> {
    vec![
        Template {
            label: "Blank",
            name: "",
            ty: BlockType {
                params: vec![param("who", ParamKind::Text, "world")],
                ..Default::default()
            },
            script: include_str!("templates/blank.py"),
        },
        Template {
            label: "Jira tickets",
            name: "jira",
            ty: BlockType {
                label: "Jira tickets".into(),
                icon: "ticket".into(),
                script: "jira.py".into(),
                params: vec![
                    param(
                        "jql",
                        ParamKind::Text,
                        "assignee = currentUser() AND resolution = Unresolved ORDER BY updated DESC",
                    ),
                    param("limit", ParamKind::Number, "10"),
                ],
                env: BTreeMap::from([
                    ("JIRA_URL".into(), "https://jiraims.rm.imshealth.com".into()),
                    ("JIRA_TOKEN".into(), "keychain:buffer-jira".into()),
                ]),
                ..Default::default()
            },
            script: include_str!("templates/jira.py"),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::model::valid_name;

    #[test]
    fn templates_are_usable() {
        for t in all() {
            assert!(t.name.is_empty() || valid_name(t.name), "{}", t.label);
            assert!(!t.script.trim().is_empty(), "{}", t.label);
            assert!(!t.ty.script.is_empty(), "{}", t.label);
        }
    }
}
