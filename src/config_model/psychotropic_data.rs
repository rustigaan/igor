use anyhow::{anyhow, Result};
use ahash::AHashMap;
use serde::Deserialize;
use super::psychotropic::{NicheTriggers, PsychotropicConfig};

#[derive(Deserialize,Debug)]
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

#[derive(Debug)]
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

    fn get(&self, key: &str) -> Option<&impl NicheTriggers> {
        self.0.get(key)
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
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