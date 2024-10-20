use std::mem::swap;
use anyhow::{anyhow, Result};
use ahash::{AHashMap, AHashSet};
use log::debug;
use serde::{Deserialize, Serialize};
use crate::config_model::use_thundercloud_config_data::UseThundercloudConfigData;
use crate::config_model::UseThundercloudConfig;
use crate::file_system::ConfigFormat;
use super::psychotropic::{NicheTriggers, PsychotropicConfig};

#[derive(Deserialize,Serialize,Debug,Clone)]
#[serde(rename_all = "kebab-case")]
pub struct NicheCueData {
    name: String,
    use_thundercloud: Option<UseThundercloudConfigData>,
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

#[derive(Deserialize,Serialize,Debug)]
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

    fn use_thundercloud(&self) -> Option<&impl UseThundercloudConfig> {
        self.niche_cue.use_thundercloud.as_ref()
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

impl PsychotropicConfigIndex {
    pub fn from_str(body: &str, config_format: ConfigFormat) -> Result<Self> {
        let data: PsychotropicConfigData = match config_format {
            ConfigFormat::TOML => toml::from_str(body)?,
            ConfigFormat::YAML => {
                let result = serde_yaml::from_str(body)?;
                #[cfg(test)]
                crate::test_utils::log_toml("Psychotropic Config", &result)?;
                result
            }
        };

        data_to_index(&data)
    }
}

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

pub fn data_to_index(data: &PsychotropicConfigData) -> Result<PsychotropicConfigIndex> {
    let mut barriers = AHashSet::new();
    let mut current_barrier = "#".to_string();
    let mut current_barrier_wait_for = Vec::new();
    let mut in_block = None;
    barriers.insert(current_barrier.clone());
    let mut index: AHashMap<String, NicheTriggersData> = AHashMap::new();
    for cue in &data.cues {
        let cue_name = cue.name();
        if let Some(_) = cue_name.strip_prefix("#") {
            if barriers.contains(&cue_name) {
                return Err(anyhow!("Barrier appears multiple times in psychotropic config: {:?}", &cue_name));
            }
            if in_block.is_some() {
                let mut name = cue_name.clone();
                swap(&mut name, &mut current_barrier);
                let previous_barrier_name = name.clone();
                let mut wait_for = Vec::new();
                swap(&mut wait_for, &mut current_barrier_wait_for);
                let barrier_cue = NicheCueData { name, wait_for, use_thundercloud: None };
                index.insert(previous_barrier_name, NicheTriggersData::new(barrier_cue));
            } else {
                current_barrier = cue_name.clone();
            }
            barriers.insert(current_barrier.clone());
            in_block = Some(current_barrier.clone());
            continue;
        }
        if index.contains_key(&cue_name) {
            return Err(anyhow!("Niche appears multiple times in psychotropic config: {:?}", &cue.name));
        }
        current_barrier_wait_for.push(cue_name.clone());
        let mut wait_for = cue.wait_for();
        let mut wait_for_extended;
        if let Some(barrier_name) = &in_block {
            wait_for_extended = wait_for.to_vec();
            wait_for_extended.push(barrier_name.clone());
            wait_for = wait_for_extended.as_slice();
        }
        for dep in wait_for {
            if let Some(niche_trigger) = index.get_mut(dep) {
                niche_trigger.triggers.push(cue.name())
            } else {
                let trivial = NicheCueData { name: dep.clone(), wait_for: Vec::new(), use_thundercloud: None };
                let mut niche_trigger = NicheTriggersData::new(trivial);
                niche_trigger.triggers.push(cue.name());
                index.insert(dep.clone(), niche_trigger);
            }
        }
        index.insert(cue.name().to_string(), NicheTriggersData::new(cue.clone()));
    }
    Ok(PsychotropicConfigIndex(index))
}

pub fn empty() -> PsychotropicConfigIndex {
    PsychotropicConfigIndex(AHashMap::new())
}