use anyhow::{anyhow, Result};
use ahash::AHashMap;
use serde::Deserialize;
use super::psychotropic::{NicheCue, PsychotropicConfig};

#[derive(Deserialize,Debug)]
#[serde(rename_all = "kebab-case")]
pub struct NicheCueData {
    name: String,
    #[serde(default)]
    wait_for: Vec<String>,
}

impl NicheCue for NicheCueData {
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
pub struct PsychotropicConfigIndex(AHashMap<String,NicheCueData>);

impl PsychotropicConfig for PsychotropicConfigIndex {
    type NicheCueImpl = NicheCueData;

    fn get(&self, key: &str) -> Option<&impl NicheCue> {
        self.0.get(key)
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

pub fn data_to_index(data: PsychotropicConfigData) -> Result<PsychotropicConfigIndex> {
    let mut index = AHashMap::new();
    for cue in data.cues {
        if index.contains_key(&cue.name()) {
            return Err(anyhow!("Niche appears multiple times in psychotropic config: {:?}", &cue.name))
        }
        for dep in cue.wait_for() {
            if !index.contains_key(dep) {
                let trivial = NicheCueData { name: dep.clone(), wait_for: Vec::new() };
                index.insert(dep.clone(), trivial);
            }
        }
        index.insert(cue.name().to_string(), cue);
    }
    Ok(PsychotropicConfigIndex(index))
}

pub fn empty() -> PsychotropicConfigIndex {
    PsychotropicConfigIndex(AHashMap::new())
}