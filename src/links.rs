use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ManagedLink {
    pub ifindex: u32,
    pub ifname: String,
    pub source: String,
    pub connection_id: Option<String>,
    pub connection_uuid: Option<String>,
    pub device_type: Option<String>,
}

impl ManagedLink {
    pub fn short(&self) -> String {
        format!("{} (ifindex {})", self.ifname, self.ifindex)
    }
}

#[derive(Debug, Default)]
pub struct LinkDiff {
    pub added: Vec<ManagedLink>,
    pub removed: Vec<ManagedLink>,
    pub unchanged: Vec<ManagedLink>,
}

pub fn diff_links(old_links: &[ManagedLink], new_links: &[ManagedLink]) -> LinkDiff {
    let old: BTreeMap<(u32, String), ManagedLink> = old_links
        .iter()
        .cloned()
        .map(|link| ((link.ifindex, link.ifname.clone()), link))
        .collect();
    let new: BTreeMap<(u32, String), ManagedLink> = new_links
        .iter()
        .cloned()
        .map(|link| ((link.ifindex, link.ifname.clone()), link))
        .collect();
    let mut all_keys = BTreeSet::new();
    all_keys.extend(old.keys().cloned());
    all_keys.extend(new.keys().cloned());

    let mut diff = LinkDiff::default();
    for key in all_keys {
        match (old.get(&key), new.get(&key)) {
            (Some(old_link), Some(new_link)) => {
                if old_link == new_link {
                    diff.unchanged.push(new_link.clone())
                } else {
                    diff.removed.push(old_link.clone());
                    diff.added.push(new_link.clone());
                }
            }
            (Some(old_link), None) => diff.removed.push(old_link.clone()),
            (None, Some(new_link)) => diff.added.push(new_link.clone()),
            (None, None) => {}
        }
    }
    diff
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diffs_links() {
        let a = ManagedLink {
            ifindex: 1,
            ifname: "eth0".into(),
            source: "resolved".into(),
            connection_id: None,
            connection_uuid: None,
            device_type: None,
        };
        let b = ManagedLink {
            ifindex: 2,
            ifname: "wlan0".into(),
            source: "resolved".into(),
            connection_id: None,
            connection_uuid: None,
            device_type: None,
        };
        let diff = diff_links(&[a.clone()], &[a, b.clone()]);
        assert_eq!(diff.added, vec![b]);
        assert_eq!(diff.unchanged.len(), 1);
    }
}
