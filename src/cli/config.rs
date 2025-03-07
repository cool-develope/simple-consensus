use std::fs::File;
use std::io::Read;
use std::error::Error;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub listen_on_port: u16,
    pub bootstrap_nodes: Vec<String>,
    pub secret_key: String,    
}

impl Config { 
    pub fn new(path: &str) -> Result<Self, Box<dyn Error>> {
        let mut file = File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let config: Config = serde_yaml::from_str(&contents)?;

        Ok(config)
    }
}