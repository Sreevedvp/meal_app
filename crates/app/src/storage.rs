use super::*;
#[derive(Deserialize)]
struct Session {
    storage_scope: String,
    production: bool,
}

pub async fn initialize() -> Result<bool, String> {
    let required = option_env!("MEAL_REQUIRE_AUTH") == Some("true");
    let endpoint = default_api();
    let session = if endpoint.is_empty() {
        None
    } else {
        match reqwest::Client::new()
            .get(format!("{endpoint}/api/session"))
            .timeout(std::time::Duration::from_secs(8))
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => r.json::<Session>().await.ok(),
            _ => None,
        }
    };
    if required && !session.as_ref().is_some_and(|s| s.production) {
        return Err(
            "Your sign-in could not be confirmed. Sign in again to access saved plans.".into(),
        );
    }
    let session = session.unwrap_or(Session {
        storage_scope: "development".into(),
        production: false,
    });
    let scope = serde_json::to_string(&session.storage_scope)
        .map_err(|_| "Could not initialize device storage.")?;
    document::eval(&format!(r#"
        const scope={scope};
        const prefix=scope==='development'?'':scope+':';
        window.deccanStorage=Object.freeze({{
            getItem:key=>localStorage.getItem(prefix+key),
            setItem:(key,value)=>localStorage.setItem(prefix+key,value),
            clear:()=>{{for(const key of ['deccan-fitness-v1','deccan-planner-v1']) localStorage.removeItem(prefix+key);}}
        }});
        addEventListener('pageshow',event=>{{if(event.persisted)location.reload();}});
        return true;
    "#)).join::<bool>().await.map_err(|_|"Could not initialize device storage.".to_string())?;
    Ok(session.production)
}
