//! Which code mod owns a block, so a turned-off mod's blocks stay as they are.
//!
//! Options > Mods turns a code mod off by skipping its hooks; its content stays registered and
//! its blocks stay in the world. Without the mod's hooks the engine would run the template's own
//! behaviour on them (a random tick, a scheduled tick, a shape update) and could rewrite or
//! remove them. The level asks [`owner_off`] first and leaves such a block untouched, so it is
//! there as it was when the mod is turned on again.
use std::{collections::HashMap, sync::RwLock};

/// The namespaces of the code mods' content, each with its mod's dispatch index
/// (`mod_index`). Content-only mods have no code to turn off and are not in it.
static OWNERS: RwLock<Option<HashMap<String, usize>>> = RwLock::new(None);

/// Records the code mods among `mods` (id, owned namespaces): those with an installed dispatch
/// entry. Returns the table it replaced, for a test to put back.
pub(super) fn set<'a>(
    mods: impl IntoIterator<Item = (&'a str, &'a [String])>,
) -> Option<HashMap<String, usize>> {
    let owners: HashMap<_, _> = mods
        .into_iter()
        .filter_map(|(id, namespaces)| Some((mod_api::__private::mod_index(id)?, namespaces)))
        .flat_map(|(index, namespaces)| namespaces.iter().map(move |ns| (ns.clone(), index)))
        .collect();
    let owners = (!owners.is_empty()).then_some(owners);
    std::mem::replace(&mut *OWNERS.write().unwrap_or_else(|e| e.into_inner()), owners)
}

/// Whether the block named `name` belongs to a code mod that is turned off now.
pub(super) fn owner_off(name: &str) -> bool {
    let Some((namespace, _)) = name.split_once(':') else {
        return false;
    };
    if namespace == "minecraft" {
        return false;
    }
    OWNERS
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|owners| owners.get(namespace))
        .is_some_and(|&index| !mod_api::__private::mod_index_on(index))
}

impl super::GameLevel<'_> {
    /// Whether `state` is a block of a code mod that is turned off now ([`owner_off`]).
    pub(super) fn block_owner_off(&self, state: horizon_world::registry::BlockStateId) -> bool {
        self.registry
            .state(state)
            .is_some_and(|block| owner_off(&block.name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Quiet;
    #[mod_api::__subscriptions]
    impl mod_api::Hooks for Quiet {}

    #[test]
    fn a_turned_off_code_mods_blocks_are_left_alone_until_it_is_on_again() {
        let mods: mod_api::Mods = vec![mod_api::ModEntry::new("frost", Box::new(Quiet))].into();
        mod_platform::testing::with_mods(mods, || {
            let frost = ["frost".to_string(), "frost_extra".to_string()];
            let pack = ["pack".to_string()];
            let before = set([("frost", &frost[..]), ("pack", &pack[..])]);

            assert!(!owner_off("frost:ice"), "an enabled mod's block ticks");
            assert!(mod_api::__private::set_mod_enabled("frost", false));
            assert!(owner_off("frost:ice"));
            assert!(owner_off("frost_extra:slush"), "every owned namespace");
            assert!(!owner_off("minecraft:ice"));
            assert!(!owner_off("pack:stone"), "a content-only mod has nothing to turn off");
            assert!(!owner_off("unknown:stone"));
            assert!(mod_api::__private::set_mod_enabled("frost", true));
            assert!(!owner_off("frost:ice"));

            set_back(before);
        });
    }

    fn set_back(owners: Option<HashMap<String, usize>>) {
        *OWNERS.write().unwrap_or_else(|e| e.into_inner()) = owners;
    }
}
