use serde::{Deserialize};

#[derive(Deserialize, Debug)]
#[serde(rename_all="camelCase")]
pub struct SummonerDto {
    pub account_id : String,    // encrypted account id
    pub profile_icon_id : i32,  // id of summoner icon for account
    pub revision_date : i64,    // date of last modification as epoch millis
    pub name : String,          // summoner name
    pub id : String,            // encrypted summoner id
    pub puuid : String,         // encrypted puuid
    pub summoner_level : i64    // level of summoner
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    ByAccount = 0,
    ByName,
}

pub fn by_account_uri(encrypted_account_id : &str)->String {
    format!("/lol/summoner/v4/summoners/by-account/{}", encrypted_account_id)
}

pub fn by_name_uri(summoner_name : &str)->String {
    format!("/lol/summoner/v4/summoners/by-name/{}", summoner_name)
}