use anyhow::{anyhow, Result};
use ahash::{AHashMap, AHashSet};
use log::debug;
use serde::Deserialize;
use super::psychotropic::{NicheTriggers, PsychotropicConfig};

#[derive(Deserialize,Debug,Clone)]
#[serde(rename_all = "kebab-case")]
pub struct NicheCueData {
    name: String,
    #[serde(default)]
    wait_for: Vec<String>,
}

impl NicheCueData {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn wait_for(&self) -> &[String] {
        &self.wait_for
    }
}

#[derive(Deserialize,Debug)]
pub struct PsychotropicConfigData {
    cues: Vec<NicheCueData>
}

#[derive(Debug, Clone)]
pub struct NicheTriggersData {
    niche_cue: NicheCueData,
    triggers: Vec<String>,
}

impl NicheTriggers for NicheTriggersData {
    fn name(&self) -> String {
        self.niche_cue.name()
    }

    fn wait_for(&self) -> &[String] {
        self.niche_cue.wait_for()
    }

    fn triggers(&self) -> &[String] {
        &self.triggers
    }
}

impl NicheTriggersData {
    fn new(niche_cue: NicheCueData) -> Self {
        NicheTriggersData {
            niche_cue,
            triggers: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct PsychotropicConfigIndex(AHashMap<String, NicheTriggersData>);

impl PsychotropicConfig for PsychotropicConfigIndex {
    type NicheTriggersImpl = NicheTriggersData;

    fn independent(&self) -> AHashSet<String> {
        let mut candidates = AHashSet::new();
        let mut excludes = AHashSet::new();
        for triggers in self.0.values() {
            let wait_for = triggers.wait_for();
            let niche = triggers.name();
            debug!("Is independent? {:?}: {:?}", &niche, wait_for);
            if wait_for.is_empty() {
                if !excludes.contains(&niche) {
                    candidates.insert(niche);
                }
            } else {
                debug!("Exclude from independent: {:?}", &niche);
                excludes.insert(niche);
                for dep in wait_for {
                    if !excludes.contains(dep) {
                        candidates.insert(dep.to_string());
                    }
                }
            }
        }
        for exclude in excludes.iter() {
            candidates.remove(exclude);
        }
        candidates
    }

    fn get(&self, key: &str) -> Option<&Self::NicheTriggersImpl> {
        self.0.get(key)
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn values(&self) -> Vec<Self::NicheTriggersImpl> {
        self.0.values().into_iter().map(Self::NicheTriggersImpl::clone).collect()
    }
}

pub fn data_to_index(data: PsychotropicConfigData) -> Result<PsychotropicConfigIndex> {
    let mut index: AHashMap<String, NicheTriggersData> = AHashMap::new();
    for cue in data.cues {
        if index.contains_key(&cue.name()) {
            return Err(anyhow!("Niche appears multiple times in psychotropic config: {:?}", &cue.name))
        }
        for dep in cue.wait_for() {
            if let Some(niche_trigger) = index.get_mut(dep) {
                niche_trigger.triggers.push(cue.name())
            } else {
                let trivial = NicheCueData { name: dep.clone(), wait_for: Vec::new() };
                let mut niche_trigger = NicheTriggersData::new(trivial);
                niche_trigger.triggers.push(cue.name());
                index.insert(dep.clone(), niche_trigger);
            }
        }
        index.insert(cue.name().to_string(), NicheTriggersData::new(cue));
    }
    Ok(PsychotropicConfigIndex(index))
}

pub fn empty() -> PsychotropicConfigIndex {
    PsychotropicConfigIndex(AHashMap::new())
}