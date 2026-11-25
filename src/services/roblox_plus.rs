//! Public subscription badge from Roblox's profile endpoint.
use crate::core::Core;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Query<'a> {
    user_ids: &'a [u64],
    fields: [&'static str; 1],
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Profiles {
    profile_details: Vec<Badge>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Badge {
    user_id: serde_json::Value,
    has_roblox_subscription: Option<bool>,
}
pub fn refresh(core: &Core) {
    let ids: Vec<_> = core.accounts().iter().map(|a| a.user_id).filter(|id| *id > 0).collect();
    let mut updates = HashMap::new();
    for batch in ids.chunks(100) {
        let query = Query { user_ids: batch, fields: ["hasRobloxSubscription"] };
        let Ok(mut response) =
            crate::roblox::agent().post("https://apis.roblox.com/user-profile-api/v1/user/profiles/get-profiles").send_json(&query)
        else {
            continue;
        };
        if response.status().as_u16() == 429 {
            break;
        }
        if let Ok(result) = response.body_mut().read_json::<Profiles>() {
            for badge in result.profile_details {
                if let (Some(id), Some(value)) = (crate::import::as_u64(&badge.user_id), badge.has_roblox_subscription) {
                    updates.insert(id, value);
                }
            }
        }
    }
    if !updates.is_empty() {
        let _ = core.edit(|data| {
            for a in &mut data.accounts {
                if let Some(v) = updates.get(&a.user_id) {
                    a.fields.insert("roblox_plus".into(), v.to_string());
                }
            }
        });
    }
}
