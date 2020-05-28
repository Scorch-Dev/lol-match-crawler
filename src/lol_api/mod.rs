
// external uses
use reqwest::blocking::{Client, Response};


// my mods
mod services;
use services::summoner_v4;
mod errors;
pub use errors::*;

#[derive(Debug)]
pub struct Context {
    api_key : String,
    client : Client
}

// used to identify region
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum Region {
    Na1 = 0,
}

impl Context {

    pub fn new(api_key : &str) -> Context {
        Context{
            api_key : api_key.to_string(),
            client : Client::new(),
        }
    }

    pub fn query_summoner_v4_by_summoner_name(
        &mut self, region_code : Region, summoner_name : &str)->Result<summoner_v4::SummonerDto>{

        let uri = Self::region_uri(region_code) + &summoner_v4::by_name_uri(summoner_name);
        let response = self.send_query(&uri)?;
        let dto = response.json::<summoner_v4::SummonerDto>()?;
        Ok(dto)
    }

    fn send_query(&mut self, uri : &str)->Result<Response> {
        let r = self.client.get(uri)
            .header("X-Riot-Token", &self.api_key)
            .send()?;
        Ok(r)
    }

    /*
    fn get_endpoint_mut(&mut self, region_code : Region, endpoint_id : Service)->&mut Endpoint {
        self.endpoints.get_mut(&region_code).unwrap().get_mut(&endpoint_id).unwrap()
    }
    */
    fn region_uri(region : Region)->String {
        format!("https://{:?}.api.riotgames.com", region)
    }
}