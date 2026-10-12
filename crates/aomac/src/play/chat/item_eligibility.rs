//! Catalogue item wear eligibility: GC 10052904 -> item vtable 10162a24+5c
//! (10081815), criteria 100095a5/100065d8, final status 100805b5.
use anyhow::{bail, ensure, Result};
use crate::play::zone::Zone;
use ao_formats::dynel_visual::{parse_item_template, ItemTemplate};

#[derive(Default)]
pub(crate) struct PerkEquipment {
    worn: [Option<ao_net::n3::world::AcgItem>; 16],
    armor: [[i32; 3]; 16],
    len: usize,
}
impl PerkEquipment {
    pub(crate) fn armor(&self) -> &[[i32; 3]] { &self.armor[..self.len] }
    pub(crate) fn refresh(&mut self, store: &ao_rdb::RecordStore, zone: &Zone) -> Result<bool> {
        let worn = std::array::from_fn(|i| zone.inventory.get(&(i as u32 + 16)).map(|e| e.item));
        if self.worn == worn { return Ok(false); }
        let mut armor = [[0; 3]; 16];
        let mut len = 0;
        for item in worn.iter().flatten() {
            let low = ao_formats::dynel_visual::item_template(store, item.low_id as u32)?.ok_or_else(|| anyhow::anyhow!("missing equipped item {}", item.low_id))?;
            let high_id = if item.high_id == 0 { item.low_id } else { item.high_id };
            let template = if high_id == item.low_id { low } else {
                let high = ao_formats::dynel_visual::item_template(store, high_id as u32)?.ok_or_else(|| anyhow::anyhow!("missing equipped item {high_id}"))?;
                ao_formats::dynel_visual::interpolate_item_templates(&low, &high, item.level)?
            };
            armor[len] = [template.stat(0).unwrap_or(0), template.stat(30).unwrap_or(0), template.stat(702).unwrap_or(0)];
            len += 1;
        }
        self.worn = worn;
        self.armor = armor;
        self.len = len;
        Ok(true)
    }
}

pub(crate) struct PerkCriteria {
    item: ItemTemplate,
    wear: Vec<[i32; 3]>,
}
impl PerkCriteria {
    pub(crate) fn from_record(record: &[u8]) -> Result<Self> {
        let item = parse_item_template(record)?;
        let mut data = super::template_spells::data(record, u32::MAX)?;
        let wear = data.criteria.iter().rposition(|e| e.1 == 6).map(|i| data.criteria.swap_remove(i).2).unwrap_or_default();
        Ok(Self { item, wear })
    }
    /// GC 10052a24: profession enumeration is an any-match scan, not evaluation
    /// of the complete wear expression. No authored profession equality accepts all.
    pub(crate) fn profession_matches(&self, profession: i32) -> bool {
        let mut found = false;
        for &[stat, value, op] in &self.wear {
            if stat == 60 && op == 0 {
                found = true;
                if value == profession { return true; }
            }
        }
        !found
    }
    /// `run_flags` is SimpleChar+138, NOT catalogue Perk_t+0xc flags.
    /// Armor triples are cached equipped slots 16..32: [class stat0, Flags30, stat702].
    pub(crate) fn eligible_with_state(&self, run_flags: u32, zone: &Zone, armor: &[[i32; 3]], completed: &[u32]) -> Result<bool> {
        let flags = self.item.stat(30).unwrap_or(0) as u32;
        if flags & 4 == 0 { return Ok(false); }
        if run_flags & 2 != 0 && flags & 0x10000000 == 0 && !matches!(self.item.stat(76), Some(3 | 5)) { return Ok(false); }
        if !evaluate(&self.wear, zone, completed)? { return Ok(false); }
        // Native virtual+14 checks the class mask (stat0), not Flags30.
        if self.item.stat(0).unwrap_or(0) & 0x100 != 0 {
            let social = self.item.stat(702).unwrap_or(0);
            if armor.iter().any(|&[_, flags, group]| flags as u32 & 0x10000000 == 0 && group != social) { return Ok(false); }
        }
        // This is specifically the native status == 2 branch, never nonzero.
        Ok(true)
    }
    /// GC10080abc sets run bit2 for equipped item class0x100 on slots below0x30.
    pub(crate) fn eligible(&self, zone: &Zone, armor: &[[i32; 3]], completed: &[u32]) -> Result<bool> {
        let run_flags = if armor.iter().any(|c| c[0] & 0x100 != 0) { 2 } else { 0 };
        self.eligible_with_state(run_flags, zone, armor, completed)
    }
}

fn evaluate(criteria: &[[i32; 3]], zone: &Zone, completed: &[u32]) -> Result<bool> {
    // GC uses a 100-byte postfix stack. Check before writing; do not reproduce
    // its unchecked overflow on malformed item records.
    let mut stack = [false; 100];
    let mut len = 0;
    for &[stat, value, op] in criteria {
        match op {
            19 => continue,
            3 | 4 => {
                ensure!(len >= 2, "criterion binary operator without operands");
                len -= 1;
                stack[len - 1] = if op == 3 { stack[len - 1] || stack[len] } else { stack[len - 1] && stack[len] };
                continue;
            }
            42 => {
                ensure!(len != 0, "criterion not without operand");
                stack[len - 1] = !stack[len - 1];
                continue;
            }
            _ => {}
        }
        let actual = zone.skill_value(stat as u32).unwrap_or(1_234_567_890);
        let result = match op {
            0 => actual == value,
            1 => actual < value,
            2 => actual > value,
            22 => actual & value == value,
            24 => actual != value,
            107 => actual & value == 0,
            93 => completed.contains(&(value as u32)),
            103 => !completed.contains(&(value as u32)),
            // Native switch cases deliberately produce true, not unknown.
            45 | 73 | 74 | 98 | 104 | 108 | 109 | 115..=118 | 127 | 128 | 134..=136 => true,
            130 => actual == zone.skill_value(value as u32).unwrap_or(1_234_567_890),
            131 => actual < zone.skill_value(value as u32).unwrap_or(1_234_567_890),
            132 => actual > zone.skill_value(value as u32).unwrap_or(1_234_567_890),
            133 => actual != zone.skill_value(value as u32).unwrap_or(1_234_567_890),
            _ => bail!("unsupported native wear criterion [{stat}, {value}, {op}]"),
        };
        ensure!(len < stack.len(), "criterion stack exceeds native capacity");
        stack[len] = result;
        len += 1;
    }
    Ok(stack[..len].iter().all(|&v| v))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_postfix_and_status_guards() {
        let mut zone = Zone::new(1);
        zone.stats.extend([(54, 20), (60, 1), (389, 24)]);
        let item = ItemTemplate { kind: 0xc73d, stats: vec![(30, 4), (76, 1)], name: None, sounds: vec![] };
        let mut check = PerkCriteria { item, wear: vec![[54, 19, 2], [60, 2, 0], [60, 1, 0], [0, 0, 3], [0, 0, 4], [389, 16, 22]] };
        assert!(check.profession_matches(1));
        assert!(!check.profession_matches(3));
        assert!(check.eligible(&zone, &[], &[]).unwrap());
        assert!(!check.eligible(&zone, &[[0x100, 4, 1]], &[]).unwrap());
        check.item.stats.push((0, 0x100));
        assert!(!check.eligible_with_state(0, &zone, &[[0x100, 4, 1]], &[]).unwrap());
        assert!(check.eligible_with_state(0, &zone, &[[0x100, 0x10000004, 1]], &[]).unwrap());
        check.wear = vec![[0, 0, 3]];
        assert!(check.eligible(&zone, &[], &[]).is_err());
    }
    #[test]
    fn native_catalogue_wear_criteria() {
        let dir = ao_gui::client_dir();
        if !dir.join("cd_image/rdb.db").exists() { return; }
        let store = ao_rdb::RecordStore::open(&dir).unwrap();
        // Catalogue perk161 -> item212164: profession14, level>29, completed160.
        let raw = store.get(1_000_020, 212164).unwrap().unwrap();
        let check = PerkCriteria::from_record(&raw).unwrap();
        let mut zone = Zone::new(1);
        zone.stats.extend([(60, 14), (54, 29)]);
        assert!(check.profession_matches(14));
        assert!(!check.profession_matches(1));
        assert!(!check.eligible_with_state(0, &zone, &[], &[160]).unwrap());
        zone.stats.insert(54, 30);
        assert!(!check.eligible_with_state(0, &zone, &[], &[]).unwrap());
        assert!(check.eligible_with_state(0, &zone, &[], &[160]).unwrap());
        // Every catalogue wear stream must be supported, including dormant rows.
        for id in store.ids(1_000_036).unwrap() {
            let def = store.get(1_000_036, id).unwrap().unwrap();
            let item = u32::from_le_bytes(def[..4].try_into().unwrap());
            if let Some(raw) = store.get(1_000_020, item).unwrap() {
                let check = PerkCriteria::from_record(&raw).unwrap_or_else(|e| panic!("perk{id} item{item}: {e}"));
                evaluate(&check.wear, &zone, &[]).unwrap_or_else(|e| panic!("perk{id} item{item}: {e}"));
            }
        }
    }
}
