use serde::{Deserialize};

#[derive(Deserialize, Debug)]
#[serde(rename_all="camelCase")]
pub struct SummonerDto {
    account_id : String,    // encrypted account id
    profile_icon_id : i32,  // id of summoner icon for account
    revision_date : i64,    // date of last modification as epoch millis
    name : String,          // summoner name
    id : String,            // encrypted summoner id
    puuid : String,         // encrypted puuid
    summoner_level : i64    // level of summoner
}

pub fn by_account_uri(encrypted_account_id : &str)->String {
    format!("/lol/summoner/v4/summoners/by-account/{}", encrypted_account_id)
}

pub fn by_name_uri(summoner_name : &str)->String {
    format!("/lol/summoner/v4/summoners/by-name/{}", summoner_name)
}

pub fn by_puuid_uri(encrypted_puuid : &str)->String{
    format!("/lol/summoner/v4/summoners/by-puuid/{}", encrypted_puuid)
}

pub fn by_summoner_id_uri(encrypted_summoner_id : &str)->String{
    format!("/lol/summoner/v4/summoners/{}", encrypted_summoner_id)
}