use super::*;

#[derive(Clone, Serialize, Deserialize, PartialEq)]
struct SavedEntry {
    record: StoredFitnessPlan,
    completed: Vec<usize>,
    checked: Vec<String>,
    remote: bool,
}
#[derive(Clone, Default, Deserialize)]
struct OnlinePage {
    items: Vec<OnlineSummary>,
    has_more: bool,
}
#[derive(Clone, Deserialize)]
struct OnlineSummary {
    id: String,
    start_date: String,
    days: u32,
}
#[derive(Clone)]
pub struct PlannerSession {
    active: Option<SavedEntry>,
    saved: Vec<SavedEntry>,
    loaded: bool,
    busy: bool,
    editing: bool,
    start: String,
    horizon: String,
    note: String,
    error: String,
}
impl Default for PlannerSession {
    fn default() -> Self {
        Self {
            active: None,
            saved: vec![],
            loaded: false,
            busy: false,
            editing: false,
            start: String::new(),
            horizon: "month".into(),
            note: String::new(),
            error: String::new(),
        }
    }
}
fn check_entry(mut entry: SavedEntry) -> Result<SavedEntry, String> {
    if entry.record.plan.catalogue_version != entry.record.catalogue.version {
        return Err("Saved catalogue does not match this plan.".into());
    }
    let mut verified = finish_fitness_plan(
        &entry.record.catalogue,
        &entry.record.plan.request,
        entry.record.plan.meals.clone(),
    )?;
    verified.id = entry.record.plan.id;
    entry.completed.retain(|i| *i < verified.meals.len());
    entry.record.plan = verified;
    Ok(entry)
}
async fn save_local(entries: &[SavedEntry]) -> Result<(), String> {
    let json = serde_json::to_string(entries).map_err(|e| e.to_string())?;
    let literal = serde_json::to_string(&json).map_err(|e| e.to_string())?;
    document::eval(&format!(
        "window.deccanStorage.setItem('deccan-planner-v1', {literal}); return true;"
    ))
    .join::<bool>()
    .await
    .map_err(|_| "Device storage is unavailable or full.".to_string())?;
    Ok(())
}
async fn api_plan(request: reqwest::RequestBuilder) -> Result<StoredFitnessPlan, String> {
    let response = request
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|_| "Could not reach online storage. Please try again.".to_string())?;
    if !response.status().is_success() {
        return Err(response
            .json::<ApiError>()
            .await
            .map(|e| e.message)
            .unwrap_or_else(|_| "The server could not save or load this plan.".into()));
    }
    response
        .json()
        .await
        .map_err(|_| "The server returned an unreadable plan.".into())
}
async fn persist_calendar(
    mut entry: SavedEntry,
    saved: Vec<SavedEntry>,
    endpoint: String,
    online: bool,
) -> Result<(SavedEntry, Vec<SavedEntry>, String), String> {
    let original_id = entry.record.plan.id.clone();
    let content_unchanged = saved.iter().any(|s| s.remote && s.record == entry.record);
    let input = PlannerSaveRequest {
        revision: entry.remote.then_some(entry.record.revision),
        request: entry.record.plan.request.clone(),
        meals: entry.record.plan.meals.clone(),
    };
    let mut note = String::new();
    if entry.remote && !original_id.is_empty() {
        if !content_unchanged {
            if endpoint.is_empty() {
                return Err("Reconnect to update this saved plan. Your edits remain in the draft; you can also copy it to a new local plan.".into());
            }
            entry.record = api_plan(
                reqwest::Client::new()
                    .put(format!(
                        "{}/api/fitness-plans/{original_id}",
                        endpoint.trim_end_matches('/')
                    ))
                    .json(&input),
            )
            .await?;
        }
        if !content_unchanged {
            note = "Saved plan updated online. ".into();
        }
    } else if online && !endpoint.is_empty() {
        match api_plan(
            reqwest::Client::new()
                .post(format!(
                    "{}/api/fitness-plans",
                    endpoint.trim_end_matches('/')
                ))
                .json(&input),
        )
        .await
        {
            Ok(record) => {
                entry.record = record;
                entry.remote = true;
                note = "Calendar saved to the database. ".into();
            }
            Err(e) => note = format!("Online save was not confirmed: {e} "),
        }
    }
    if entry.record.plan.id.is_empty() {
        entry.record.plan.id =
            document::eval("return 'local-'+Date.now()+'-'+Math.random().toString(36).slice(2);")
                .join::<String>()
                .await
                .map_err(|_| "Could not create a local plan ID.".to_string())?;
    }
    let mut entries = saved.clone();
    entries.retain(|s| {
        s.record.plan.id != entry.record.plan.id
            && (original_id.is_empty() || s.record.plan.id != original_id)
    });
    entries.insert(0, entry.clone());
    entries.truncate(8);
    if let Err(e) = save_local(&entries).await {
        return Ok((
            entry.clone(),
            saved,
            format!(
                "{note}{e}{}",
                if entry.remote {
                    format!(" Online plan ID: {}", entry.record.plan.id)
                } else {
                    String::new()
                }
            ),
        ));
    }
    Ok((
        entry,
        entries,
        format!("{note}Calendar and progress saved on this device."),
    ))
}

async fn remove_calendar(
    entry: &SavedEntry,
    saved: &[SavedEntry],
    endpoint: &str,
) -> Result<Vec<SavedEntry>, String> {
    let id = &entry.record.plan.id;
    if entry.remote {
        if endpoint.is_empty() {
            return Err("Reconnect to delete this plan from online storage.".into());
        }
        let response=reqwest::Client::new().delete(format!("{}/api/fitness-plans/{id}",endpoint.trim_end_matches('/')))
            .header("If-Match",format!("\"{}\"",entry.record.revision)).timeout(std::time::Duration::from_secs(10)).send().await.map_err(|_|"Deletion could not be confirmed. Your local plan is still available; try again when connected.".to_string())?;
        if !response.status().is_success() && response.status() != reqwest::StatusCode::NOT_FOUND {
            return Err(
                "Could not delete the online plan. Your local plan is still available.".into(),
            );
        }
    }
    let entries: Vec<_> = saved
        .iter()
        .filter(|s| id.is_empty() || s.record.plan.id != *id)
        .cloned()
        .collect();
    save_local(&entries).await.map_err(|e| {
        format!(
            "{}{e} Retry deletion to update this device.",
            if entry.remote { "Deleted online. " } else { "" }
        )
    })?;
    Ok(entries)
}

#[component]
pub fn Planner(
    mut state: Signal<PlannerSession>,
    mut settings: Signal<Settings>,
    catalogue: Signal<Catalogue>,
    connected: Signal<bool>,
    settings_loaded: Signal<bool>,
) -> Element {
    let mut setup_open = use_signal(|| true);
    let mut no_allergens = use_signal(|| false);
    let mut calendar_page = use_signal(|| 0_u32);
    let mut selected_day = use_signal(|| 0_u32);
    let mut day_popup = use_signal(|| false);
    let mut week = use_signal(|| 0_u32);
    let mut show_month = use_signal(|| false);
    let mut tab = use_signal(|| "calendar".to_string());
    let mut swapping = use_signal(|| None::<usize>);
    let mut cheaper = use_signal(|| false);
    let mut restore_id = use_signal(String::new);
    let mut online_page = use_signal(OnlinePage::default);
    let mut online_offset = use_signal(|| 0_u32);
    let mut online_loaded = use_signal(|| false);
    let mut pending_delete = use_signal(|| None::<SavedEntry>);
    let mut copying = use_signal(|| false);
    let mut copy_date = use_signal(String::new);
    let mut open_online = move |id: String| {
        let endpoint = settings().endpoint;
        state.write().busy = true;
        state.write().error.clear();
        spawn(async move {
            match api_plan(reqwest::Client::new().get(format!(
                "{}/api/fitness-plans/{}",
                endpoint.trim_end_matches('/'),
                id
            )))
            .await
            {
                Ok(record) => match check_entry(SavedEntry {
                    record,
                    completed: vec![],
                    checked: vec![],
                    remote: true,
                }) {
                    Ok(entry) => {
                        state.write().active = Some(entry);
                        state.write().editing = false;
                        copying.set(false);
                        pending_delete.set(None);
                        state.write().note="Database calendar opened. Save to keep it in this device's recent plans.".into();
                        setup_open.set(false);
                        week.set(0);
                        calendar_page.set(0);
                        selected_day.set(0);
                        day_popup.set(false);
                        swapping.set(None);
                        tab.set("calendar".into());
                    }
                    Err(e) => state.write().error = e,
                },
                Err(e) => state.write().error = e,
            }
            state.write().busy = false;
        });
    };
    let mut fetch_online = move |offset: u32| {
        let endpoint = settings().endpoint;
        state.write().busy = true;
        state.write().error.clear();
        spawn(async move {
            let result = async {
                let response = reqwest::Client::new()
                    .get(format!(
                        "{}/api/fitness-plans?offset={offset}",
                        endpoint.trim_end_matches('/')
                    ))
                    .timeout(std::time::Duration::from_secs(10))
                    .send()
                    .await
                    .map_err(|_| "Could not reach saved calendars.".to_string())?;
                if !response.status().is_success() {
                    return Err(
                        "Could not load saved calendars. Check your connection or sign in again."
                            .into(),
                    );
                }
                response
                    .json::<OnlinePage>()
                    .await
                    .map_err(|_| "Could not read saved calendars.".to_string())
            }
            .await;
            match result {
                Ok(page) => {
                    online_page.set(page);
                    online_offset.set(offset);
                    online_loaded.set(true);
                }
                Err(e) => state.write().error = e,
            }
            state.write().busy = false;
        });
    };
    let mut edit_plan = move |entry: SavedEntry| {
        let request = entry.record.plan.request.clone();
        settings.write().profile = request.profile.clone();
        no_allergens.set(request.profile.allergens.is_empty());
        let mut s = state.write();
        s.start = request.start_date;
        s.horizon = if request.days == 7 { "week" } else { "month" }.into();
        s.active = Some(entry);
        s.editing = true;
        s.error.clear();
        s.note="Editing this plan. Date-only changes keep its dishes. Changes to targets or preferences rebuild meals; changed dates or meals reset progress. Apply, then save your changes.".into();
        setup_open.set(true);
        copying.set(false);
        pending_delete.set(None);
        spawn(async move {
            let _ = document::eval(
                "document.getElementById('planner-setup')?.scrollIntoView({block:'start'});",
            )
            .await;
        });
    };
    use_resource(move || async move {
        if !settings_loaded() {
            return;
        }
        if state.peek().loaded {
            return;
        }
        let today=document::eval("const d=new Date(); return `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,'0')}-${String(d.getDate()).padStart(2,'0')}`;").join::<String>().await.unwrap_or_default();
        let mut entries = vec![];
        let mut note = String::new();
        match document::eval("return window.deccanStorage.getItem('deccan-planner-v1');")
            .join::<Option<String>>()
            .await
        {
            Ok(Some(json)) => match serde_json::from_str::<Vec<SavedEntry>>(&json) {
                Ok(saved) => {
                    for entry in saved.into_iter().take(8) {
                        match check_entry(entry) {
                            Ok(e) => entries.push(e),
                            Err(_) => {
                                note =
                                    "Some saved plans could not be validated and were not loaded."
                                        .into()
                            }
                        }
                    }
                }
                Err(_) => note = "Saved plans could not be read. You can create a new plan.".into(),
            },
            Err(_) => {
                note = "Device storage is unavailable. You can still plan this session.".into()
            }
            _ => (),
        }
        setup_open.set(entries.is_empty());
        let mut s = state.write();
        s.start = today;
        s.active = entries.first().cloned();
        s.saved = entries;
        s.loaded = true;
        s.note = note;
    });
    let session = state();
    let active = session.active.clone();
    let ready = session.loaded && settings_loaded();
    let profile = settings().profile;
    let allergies_answered = no_allergens() || !profile.allergens.is_empty();
    let dirty = active
        .as_ref()
        .is_some_and(|a| !session.saved.iter().any(|saved| saved == a));
    let swaps = use_memo(move || match (swapping(), state().active) {
        (Some(i), Some(a)) => fitness_swap_options(&a.record.catalogue, &a.record.plan, i),
        _ => vec![],
    });
    rsx! {
        div {class:"page-heading",div {span {class:"eyebrow","CONSISTENCY, WITH VARIETY"} h2 {"Your meal planner"} p {"A calendar built around your targets, your food choices and your budget."}}}
        section {class:"panel planner-setup",id:"planner-setup",
            div {class:"panel-heading",div {span {class:"step","01"} h3 {"Your goals & preferences"}}
                if active.is_some() && !session.editing {button {class:"secondary",disabled:session.busy,"aria-expanded":setup_open(),onclick:move|_|setup_open.toggle(),if setup_open(){"Close setup"}else{"Edit goals & preferences"}}}
            }
            if setup_open() || active.is_none() {
            fieldset {class:"configuration",disabled:!ready||session.busy,
                p {class:"hint","Review your daily intake, monthly budget and allergens before creating your calendar. Targets are per person."}
                div {class:"input-grid planner-inputs",
                    div {label {r#for:"plan-calories","Daily calorie intake"} div {class:"unit-input",input {id:"plan-calories",r#type:"number",min:"150",step:"any",value:"{profile.daily().calories_kcal:.1}",oninput:move|e|{
                        let total=e.value().parse::<f64>().unwrap_or(0.0);let mut s=settings.write();let previous=s.profile.daily().calories_kcal;let count=s.profile.targets.len() as f64;
                        for t in &mut s.profile.targets {t.nutrition.calories_kcal=if previous>0.0{total*t.nutrition.calories_kcal/previous}else{total/count};}
                    }} span {"kcal"}}}
                    div {label {r#for:"plan-protein","Daily protein intake"} div {class:"unit-input",input {id:"plan-protein",r#type:"number",min:"3",step:"any",value:"{profile.daily().protein_g:.1}",oninput:move|e|{
                        let total=e.value().parse::<f64>().unwrap_or(0.0);let mut s=settings.write();let previous=s.profile.daily().protein_g;let count=s.profile.targets.len() as f64;
                        for t in &mut s.profile.targets {t.nutrition.protein_g=if previous>0.0{total*t.nutrition.protein_g/previous}else{total/count};}
                    }} span {"g"}}}
                    div {label {r#for:"plan-budget","Monthly food budget"} div {class:"unit-input",input {id:"plan-budget",r#type:"number",min:"0.01",max:"1000000",step:"0.01",value:"{profile.monthly_budget_paise as f64/100.0:.2}",onchange:move|e|settings.write().profile.monthly_budget_paise=paise(&e.value()).unwrap_or(0)} span {"INR"}}}
                    div {label {r#for:"plan-month-days","Days in your budget month"} select {id:"plan-month-days",value:"{profile.month_days}",onchange:move|e|settings.write().profile.month_days=e.value().parse().unwrap_or(30),for days in 28..=31 {option {value:"{days}","{days} days"}}}}
                }
                details {class:"planner-meal-targets",summary {"Adjust calorie & protein targets per meal"}
                    p {class:"hint","Daily changes scale these meal targets proportionally."}
                    div {class:"meal-targets",for (index,target) in profile.targets.iter().enumerate() {
                        article {class:"meal-target",div {class:"meal-title",h4 {"{target.meal.label()}"}}
                            label {r#for:"plan-meal-calories-{index}","Calories / meal"} input {id:"plan-meal-calories-{index}",r#type:"number",min:"50",max:"3000",step:"any",value:"{target.nutrition.calories_kcal:.1}",oninput:move|e|settings.write().profile.targets[index].nutrition.calories_kcal=e.value().parse().unwrap_or(0.0)}
                            label {r#for:"plan-meal-protein-{index}","Protein / meal (g)"} input {id:"plan-meal-protein-{index}",r#type:"number",min:"1",max:"250",step:"any",value:"{target.nutrition.protein_g:.1}",oninput:move|e|settings.write().profile.targets[index].nutrition.protein_g=e.value().parse().unwrap_or(0.0)}
                        }
                    }}
                }
                fieldset {class:"priority-options",legend {"What comes first?"}
                    label {class:if profile.diet_first{"choice selected"}else{"choice"},input {r#type:"radio",name:"plan-priority",checked:profile.diet_first,onchange:move|_|settings.write().profile.diet_first=true} div {strong {"Diet first"} small {"Show the budget needed for my targets"}}}
                    label {class:if !profile.diet_first{"choice selected"}else{"choice"},input {r#type:"radio",name:"plan-priority",checked:!profile.diet_first,onchange:move|_|settings.write().profile.diet_first=false} div {strong {"Budget first"} small {"Keep the plan within my allowance"}}}
                }
                div {class:"preference-row",label {r#for:"plan-diet","Food preference"} select {id:"plan-diet",value:if profile.diet==Diet::Vegetarian{"veg"}else{"meat"},onchange:move|e|settings.write().profile.diet=if e.value()=="veg"{Diet::Vegetarian}else{Diet::MeatAllowed},option {value:"veg","Vegetarian · no meat or eggs"} option {value:"meat","Meat & eggs allowed"}}}
                fieldset {class:"planner-allergens",legend {"Any food allergies?"}
                    p {class:"hint","Select every allergen to exclude, or confirm no known food allergies."}
                    label {class:"check-label",input {id:"plan-no-allergens",r#type:"checkbox",checked:no_allergens()&&profile.allergens.is_empty(),onchange:move|e|{no_allergens.set(e.checked());if e.checked(){settings.write().profile.allergens.clear();}}} "No known food allergies"}
                    div {class:"allergen-grid",for allergen in ALLERGENS {label {input {r#type:"checkbox",checked:profile.allergens.iter().any(|a|a==allergen),onchange:move|e|{no_allergens.set(false);let mut s=settings.write();s.profile.allergens.retain(|a|a!=allergen);if e.checked(){s.profile.allergens.push(allergen.to_string());}}} {allergen.replace('_'," ")}}}}
                    details {summary {"Other ingredients to avoid ({profile.excluded_ingredients.len()})"} div {class:"allergen-grid",for ingredient in catalogue().ingredients {
                        {let id=ingredient.id.clone();rsx!{label {input {r#type:"checkbox",checked:profile.excluded_ingredients.contains(&id),onchange:move|e|{let mut s=settings.write();s.profile.excluded_ingredients.retain(|i|i!=&id);if e.checked(){s.profile.excluded_ingredients.push(id.clone());}}} "{ingredient.name}"}}}
                    }}}
                }

            div {class:"planner-controls",
                div {label {r#for:"plan-start","Start date"} input {id:"plan-start",r#type:"date",min:"2000-01-01",max:"2100-12-31",value:"{session.start}",disabled:!ready||session.busy,oninput:move|e|state.write().start=e.value()}}
                div {label {r#for:"plan-length","Plan length"} select {id:"plan-length",value:"{session.horizon}",disabled:!ready||session.busy,onchange:move|e|state.write().horizon=e.value(),option {value:"week","7 days"} option {value:"month","Month · {settings().profile.month_days} days"}}}
                button {class:"primary",disabled:!ready||session.busy||!allergies_answered,onclick:move|_|{
                    let s=state();let profile=settings().profile;
                    let request=PlannerRequest {days:if s.horizon=="week"{7}else{profile.month_days},profile,start_date:s.start};
                    let original=if s.editing{s.active}else{None};let c=original.as_ref().map(|a|a.record.catalogue.clone()).unwrap_or_else(&*catalogue);state.write().busy=true;state.write().error.clear();
                    spawn(async move {
                        let result=if let Some(a)=&original {
                            if a.record.plan.request.profile==request.profile && a.record.plan.request.days==request.days {finish_fitness_plan(&c,&request,a.record.plan.meals.clone())} else {generate_fitness_plan(&c,&request)}
                        } else {generate_fitness_plan(&c,&request)};
                        let mut s=state.write();s.busy=false;
                        match result {Ok(mut plan)=>{
                            let mut entry=SavedEntry {record:StoredFitnessPlan {revision:1,plan:plan.clone(),catalogue:c},completed:vec![],checked:vec![],remote:false};
                            if let Some(old)=original {
                                entry.record.revision=old.record.revision;plan.id=old.record.plan.id.clone();
                                if plan==old.record.plan {entry.completed=old.completed;entry.checked=old.checked;}
                                entry.record.plan=plan;entry.remote=old.remote;
                                s.note="Plan changes applied to the draft. Save to update this plan.".into();
                            } else {s.note="New draft created. Save it to keep this calendar.".into();}
                            s.active=Some(entry);s.editing=false;week.set(0);calendar_page.set(0);selected_day.set(0);day_popup.set(false);swapping.set(None);tab.set("calendar".into());setup_open.set(false);},Err(e)=>s.error=e}
                    });
                },if session.busy {"Working…"}else if session.editing {"Apply plan changes"}else{"Generate plan →"}}
            }
                if session.editing {button {class:"secondary",disabled:session.busy,onclick:move|_|{
                    if let Some(a)=state().active {let request=a.record.plan.request;settings.write().profile=request.profile;state.write().start=request.start_date;state.write().horizon=if request.days==7{"week"}else{"month"}.into();}
                    state.write().editing=false;state.write().error.clear();state.write().note="Edit cancelled. The calendar is unchanged.".into();setup_open.set(false);
                },"Cancel editing"}}
                if !allergies_answered {p {class:"hint", "Answer the allergy question above to generate your plan."}}
                p {class:"hint","A seven-day budget is your monthly budget × 7 ÷ month length. No dish repeats within seven days."}
            }
            }

            div {class:"planner-target-strip",span {"{settings().profile.daily().calories_kcal:.0} kcal / day"} span {"{settings().profile.daily().protein_g:.1} g protein / day"} span {"{money(profile.monthly_budget_paise)} / month"} span {if settings().profile.diet_first {"Diet first"}else{"Budget first"}}}
        }
        if !session.error.is_empty() {div {class:"error",role:"alert","{session.error}"}}
        if !session.note.is_empty() {p {class:"storage-message",role:"status","{session.note}"}}
        if let Some(entry)=pending_delete() {
            section {class:"panel delete-confirmation",id:"delete-plan-confirmation","aria-labelledby":"delete-plan-title",
                h3 {id:"delete-plan-title","Delete this plan?"}
                p {"{entry.record.plan.request.start_date} · {entry.record.plan.request.days} days · {money(entry.record.plan.high_paise)}"}
                p {class:"hint",if entry.remote {"This deletes the saved online plan and this device's copy, including its progress. This cannot be undone."}else if entry.record.plan.id.is_empty(){"This discards the current unsaved draft and its progress."}else{"This deletes the saved plan and its progress from this device. This cannot be undone."}}
                if !session.error.is_empty() {p {class:"error","{session.error}"}}
                div {class:"plan-actions",
                    button {class:"secondary",disabled:session.busy,onclick:move|_|pending_delete.set(None),"Cancel deletion"}
                    button {class:"primary danger-primary",disabled:session.busy,onclick:move|_|{
                        let Some(entry)=pending_delete() else{return;};let saved=state().saved;let endpoint=settings().endpoint;state.write().busy=true;state.write().error.clear();
                        spawn(async move {
                            match remove_calendar(&entry,&saved,&endpoint).await {
                                Ok(entries)=>{
                                    let mut s=state.write();
                                    if s.active.as_ref().is_some_and(|a|a==&entry||(!entry.record.plan.id.is_empty()&&a.record.plan.id==entry.record.plan.id)) {s.active=None;s.editing=false;setup_open.set(true);}
                                    s.saved=entries;s.note="Plan deleted.".into();pending_delete.set(None);copying.set(false);swapping.set(None);
                                },
                                Err(e)=>state.write().error=e,
                            }
                            state.write().busy=false;
                        });
                    },if session.busy{"Deleting…"}else{"Delete permanently"}}
                }
            }
        }
        if let Some(a)=active {
            {let p=a.record.plan.clone();let c=a.record.catalogue.clone();let grid=calendar_month(&p.request.start_date,p.request.days,calendar_page()).expect("validated plan dates");let day=selected_day().min(p.request.days-1);let first_day_on_page=|page|calendar_month(&p.request.start_date,p.request.days,page).ok().and_then(|g|g.cells.iter().flatten().find_map(|c|c.plan_day)).unwrap_or(0);let previous_day=first_day_on_page(grid.page.saturating_sub(1));let next_day=first_day_on_page(grid.page+1);rsx!{
                if p.request.profile != settings().profile {div {class:"planner-stale",role:"status","Your current preferences differ from this calendar. This plan keeps the targets and exclusions it was built with. Generate a new plan to apply your current preferences."}}
                div {class:"plan-actions",
                    {let entry=a.clone();rsx!{button {class:"secondary",disabled:session.busy||session.editing,onclick:move|_|edit_plan(entry.clone()),"Edit current plan"}}}
                    button {class:"secondary",disabled:session.busy||session.editing,onclick:move|_|{copy_date.set(plan_date(&p.request.start_date,p.request.days));copying.toggle();pending_delete.set(None);},"Copy to new dates"}
                    {let entry=a.clone();rsx!{button {class:"secondary danger",disabled:session.busy,onclick:move|_|{pending_delete.set(Some(entry.clone()));copying.set(false);spawn(async move{let _=document::eval("requestAnimationFrame(()=>document.getElementById('delete-plan-confirmation')?.scrollIntoView({block:'center'}));").await;});},"Delete plan"}}}
                }
                if copying() {
                    section {class:"panel copy-plan",h3 {"Use this menu again"} p {class:"hint","Keep all dishes, portions and targets for another {p.request.days} days. The copy starts with fresh progress; your saved original stays intact."}
                        div {class:"planner-controls",
                            div {label {r#for:"copy-plan-start","New start date"} input {id:"copy-plan-start",r#type:"date",min:"2000-01-01",max:"2100-12-31",value:"{copy_date}",disabled:session.busy,oninput:move|e|copy_date.set(e.value())}}
                            button {class:"primary",disabled:session.busy,onclick:move|_|{
                                let Some(entry)=state().active else{return;};let mut request=entry.record.plan.request.clone();request.start_date=copy_date();
                                match finish_fitness_plan(&entry.record.catalogue,&request,entry.record.plan.meals.clone()) {
                                    Ok(plan)=>{settings.write().profile=request.profile;let mut s=state.write();s.active=Some(SavedEntry {record:StoredFitnessPlan {revision:1,plan,catalogue:entry.record.catalogue},completed:vec![],checked:vec![],remote:false});s.editing=false;s.error.clear();s.note="New dated copy created with fresh progress. Save this calendar to keep it.".into();copying.set(false);setup_open.set(false);week.set(0);calendar_page.set(0);selected_day.set(0);day_popup.set(false);swapping.set(None);tab.set("calendar".into());},
                                    Err(e)=>state.write().error=e,
                                }
                            },"Create dated copy"}
                            button {class:"secondary",disabled:session.busy,onclick:move|_|copying.set(false),"Cancel copy"}
                        }
                    }
                }
                div {class:"planner-summary",
                    article {span {"GROCERY ESTIMATE"} strong {"{money(p.low_paise)}–{money(p.high_paise)}"} small {"{p.request.days} days · full packs"}}
                    article {span {if p.high_paise<=p.budget_paise {"BUDGET REMAINING"}else{"ABOVE YOUR BUDGET"}} strong {"{money(p.high_paise.abs_diff(p.budget_paise))}"} small {"Period budget: {money(p.budget_paise)}"}}
                    article {span {"MEALS COMPLETED"} strong {"{a.completed.len()} / {p.meals.len()}"} small {"{p.nutrition.protein_g:.0} g protein in this plan"}}
                }
                if p.high_paise>p.budget_paise {p {class:"budget-advice","Diet first: this calendar preserves your nutrition targets and exceeds the budget. The additional allowance above uses the upper demo price estimate."}}
                div {class:"planner-toolbar",
                    div {class:"planner-tabs",button {class:if tab()=="calendar"{"active"}else{""},onclick:move|_|tab.set("calendar".into()),"Meal calendar"} button {class:if tab()=="shopping"{"active"}else{""},onclick:move|_|{tab.set("shopping".into());swapping.set(None);},"Shopping & budget"}}
                    if tab()=="shopping" {div {class:"planner-view",label {r#for:"plan-week","View"} select {id:"plan-week",value:if show_month(){"all".into()}else{week().to_string()},onchange:move|e|{let v=e.value();show_month.set(v=="all");if v!="all"{week.set(v.parse().unwrap_or(0));}swapping.set(None);},for w in 0..p.request.days.div_ceil(7) {option {value:"{w}","Week {w+1}"}} option {value:"all","Whole plan"}}}}
                }
                if tab()=="calendar" {
                    section {class:"panel planner-calendar","aria-label":"Meal calendar",
                        div {class:"calendar-heading",
                            div {span {class:"eyebrow","YOUR MEAL CALENDAR"} h3 {"{grid.title}"}}
                            div {class:"calendar-navigation",
                                button {class:"secondary","aria-label":"Previous month",disabled:grid.page==0,onclick:move|_|{calendar_page.set(grid.page.saturating_sub(1));selected_day.set(previous_day);swapping.set(None);},"←"}
                                button {class:"secondary","aria-label":"Next month",disabled:grid.page+1>=grid.pages,onclick:move|_|{calendar_page.set(grid.page+1);selected_day.set(next_day);swapping.set(None);},"→"}
                            }
                        }
                        p {class:"hint","{p.request.start_date} – {plan_date(&p.request.start_date,p.request.days-1)} · Select a date to see meals and swaps."}
                        table {class:"calendar-grid", "aria-label":"{grid.title} meal dates",
                            thead {tr {for weekday in ["Mon","Tue","Wed","Thu","Fri","Sat","Sun"] {th {scope:"col","{weekday}"}}}}
                            tbody {for row in grid.cells.chunks(7) {tr {for cell in row {td {
                                if let Some(cell)=cell {
                                    if let Some(offset)=cell.plan_day {
                                        {let meals:Vec<_>=p.meals.iter().enumerate().filter(|(_,m)|m.day==offset).collect();let completed=meals.iter().filter(|(i,_)|a.completed.contains(i)).count();let nutrition=meals.iter().fold(Nutrition::default(),|mut n,(_,m)|{n.calories_kcal+=m.nutrition.calories_kcal;n.protein_g+=m.nutrition.protein_g;n});rsx!{
                                            button {class:if offset==day{"calendar-date selected"}else if completed==meals.len(){"calendar-date complete"}else{"calendar-date"},"aria-label":"{cell.date}: {meals.len()} meals, {completed} completed","aria-pressed":offset==day,"data-plan-day":"{offset}","aria-haspopup":"dialog",onclick:move|_|{selected_day.set(offset);swapping.set(None);day_popup.set(true);},
                                                span {class:"calendar-date-number","{cell.number}"}
                                                span {class:"calendar-dishes",for (_,m) in &meals {span {"{c.recipes.iter().find(|r|r.id==m.recipe_id).map(|r|r.name.as_str()).unwrap_or_default()}"}}}
                                                span {class:"calendar-nutrition","{nutrition.calories_kcal:.0} kcal"}
                                                span {class:"calendar-progress","aria-hidden":"true",for (i,_) in &meals {span {class:if a.completed.contains(i){"checked"}else{""}}}}
                                            }
                                        }}
                                    } else {span {class:"calendar-outside","{cell.number}"}}
                                }
                            }}}}}
                        }
                        div {class:"calendar-legend",span {"● Planned meal"} span {"✓ Tap a date to open its meals"}}
                    }
                    if day_popup() {
                    dialog {class:"meal-dialog",id:"meal-dialog","aria-labelledby":"meal-dialog-title",
                        onmounted:move|_|async move {
                            // Native modal behavior keeps keyboard focus inside and restores it on close.
                            let _=document::eval(r#"
                                const dialog = document.getElementById('meal-dialog');
                                if (!dialog) return false;
                                const opener = document.activeElement;
                                const closed = new Promise(resolve => dialog.addEventListener('close', resolve, {once:true}));
                                dialog.addEventListener('click', event => {
                                    const rect = dialog.getBoundingClientRect();
                                    if (event.target === dialog && (event.clientX < rect.left || event.clientX > rect.right || event.clientY < rect.top || event.clientY > rect.bottom)) dialog.close();
                                });
                                dialog.showModal();
                                await closed;
                                if (opener?.isConnected) opener.focus({preventScroll:true});
                                return true;
                            "#).join::<bool>().await;
                            day_popup.set(false);swapping.set(None);
                        },
                        div {class:"meal-dialog-heading",
                            div {span {class:"eyebrow","YOUR DAY, PLANNED"} h2 {id:"meal-dialog-title","Day {day+1} · {plan_date(&p.request.start_date,day)}"}}
                            form {method:"dialog",button {class:"secondary",autofocus:true,"aria-label":"Close day details","Close ×"}}
                        }
                        div {class:"meal-dialog-content",
                    if !session.error.is_empty() {div {class:"error",role:"alert","{session.error}"}}
                    if let Some(index)=swapping() {
                        section {class:"panel planner-swaps",id:"planner-swaps",tabindex:"-1",
                            div {class:"panel-heading",div {h3 {"Replace {p.meals[index].meal.label()} · {plan_date(&p.request.start_date,p.meals[index].day)}"}} button {class:"secondary",onclick:move|_|swapping.set(None),"Close swaps"}}
                            p {class:"hint","Each option meets this plan's nutrition, exclusions and repeat rules. Costs below are for the entire plan after the swap."}
                            label {class:"check-label",input {r#type:"checkbox",checked:cheaper(),onchange:move|e|cheaper.set(e.checked())}"Only show cheaper swaps"}
                            div {class:"planner-swap-grid",for option in swaps().iter().filter(|o|!cheaper()||o.high_paise<p.high_paise) {
                                {let id=option.recipe_id.clone();let percent=option.portion_percent;let name=c.recipes.iter().find(|r|r.id==id).map(|r|r.name.clone()).unwrap_or_default();rsx!{
                                button {class:"planner-swap-option",disabled:session.busy,onclick:move|_|{
                                    let Some(mut entry)=state().active else{return;};
                                    match swap_fitness_meal(&entry.record.catalogue,&entry.record.plan,index,&id,percent){
                                        Ok(mut next)=>{next.id=entry.record.plan.id.clone();entry.record.plan=next;entry.completed.retain(|i|*i!=index);entry.checked.clear();let mut s=state.write();s.active=Some(entry);s.error.clear();s.note="Meal replaced and grocery costs updated. Save to keep this revision.".into();swapping.set(None);},
                                        Err(e)=>state.write().error=e,
                                    }
                                },strong {"{name}"} span {"{f64::from(percent)/100.0:.1} servings · {option.nutrition.calories_kcal:.0} kcal · {option.nutrition.protein_g:.1} g protein"} b {"Plan total: {money(option.high_paise)}"} small {if option.high_paise<p.high_paise {"Save {money(p.high_paise-option.high_paise)}"}else{"Change: +{money(option.high_paise-p.high_paise)}"}} if option.high_paise>p.budget_paise {small {"Above budget by {money(option.high_paise-p.budget_paise)}"}}}
                                }}
                            }}
                            if !swaps().iter().any(|o|!cheaper()||o.high_paise<p.high_paise) {p {class:"hint","No matching replacements found. Your targets, budget priority and repeat rule have not been relaxed."}}
                        }
                    }
                    div {class:"selected-day",id:"selected-day",
                        {let actual=p.meals.iter().filter(|m|m.day==day).fold(Nutrition::default(),|mut n,m|{n.calories_kcal+=m.nutrition.calories_kcal;n.protein_g+=m.nutrition.protein_g;n});rsx!{
                        section {class:"planner-day",div {class:"planner-day-heading",h3 {"Meals & nutrition"} span {"{actual.calories_kcal:.0} kcal · {actual.protein_g:.1} g protein"}}
                            div {class:"planner-meals",for (index,meal) in p.meals.iter().enumerate().filter(|(_,m)|m.day==day) {
                                if let Some(recipe)=c.recipes.iter().find(|r|r.id==meal.recipe_id) {
                                    article {class:if a.completed.contains(&index){"planner-meal done"}else{"planner-meal"},
                                        div {class:"planner-meal-top",span {"{meal.meal.label()}"} label {title:"Mark meal complete",input {r#type:"checkbox","aria-label":"Complete day {day+1} {meal.meal.label()}",checked:a.completed.contains(&index),disabled:session.busy,onchange:move|e|{if let Some(a)=&mut state.write().active{a.completed.retain(|i|*i!=index);if e.checked(){a.completed.push(index);}}}} "Done"}}
                                        h4 {"{recipe.name}"} div {class:"food-macros",strong {"{meal.nutrition.calories_kcal:.0} kcal"} strong {"{meal.nutrition.protein_g:.1} g protein"}}
                                        p {class:"hint","{f64::from(meal.portion_percent)/100.0:.1} servings · {recipe.minutes} min"}
                                        button {class:"secondary",disabled:session.busy,onclick:move|_|{swapping.set(Some(index));cheaper.set(false);spawn(async move{let _=document::eval("requestAnimationFrame(()=>{ const el=document.getElementById('planner-swaps'); if(el){el.scrollIntoView({block:'start'});el.focus();}});").await;});},"Swap meal"}
                                        details {summary {"Recipe & quantities"} ul {for amount in &recipe.ingredients {if let Some(i)=c.ingredients.iter().find(|i|i.id==amount.ingredient){li {"{(amount.quantity*u64::from(meal.portion_percent)).div_ceil(100)} {i.unit} {i.name}"}}}} ol {for step in &recipe.steps {li {"{step}"}}} p {"Quantities are for this portion. Preparation time may exclude soaking or fermentation."}}
                                    }
                                }
                            }}
                        }
                        }}
                    }
                        p {class:"hint dialog-save-note","Changes stay in your draft. Close this popup and save the calendar to keep them."}
                        }
                    }
                    }
                } else {
                    p {class:"hint","Buy only the additional packs needed each week. Unused quantities carry forward in the calculation. Plan fresh-food purchases around actual shelf life; delivery, fuel and waste are excluded."}
                    for (w,basket) in p.shopping_weeks.iter().enumerate().filter(|(w,_)|show_month()||*w==week() as usize) {
                        section {class:"panel shopping-week",div {class:"panel-heading",div {h3 {"Week {w+1} · {plan_date(&p.request.start_date,basket.first_day)}"}} strong {"{money(basket.low_paise)}–{money(basket.high_paise)}"}}
                            p {class:"hint","{basket.days} days · additional purchases after carryover"}
                            div {class:"cost-table",table {thead {tr {th {"Bought"} th {"Ingredient"} th {"Need"} th {"Carry in"} th {"Buy"} th {"Left over"} th {"Cost"}}} tbody {for item in &basket.items {
                                {let key=format!("{w}:{}",item.ingredient_id);let checked=a.checked.contains(&key);rsx!{
                                tr {class:if checked{"shopping-checked"}else{""},td {input {r#type:"checkbox","aria-label":"Bought week {w+1} {item.name}",checked,disabled:session.busy||item.packs==0,onchange:move|e|{if let Some(a)=&mut state.write().active{a.checked.retain(|k|k!=&key);if e.checked(){a.checked.push(key.clone());}}}}} td {"{item.name}"} td {"{item.needed} {item.unit}"} td {"{item.carry_in} {item.unit}"} td {"{item.packs} packs · {item.buy_quantity} {item.unit}"} td {"{item.leftover} {item.unit}"} td {"{money(item.low_paise)}–{money(item.high_paise)}"}}
                                }}
                            }}}}
                        }
                    }
                    details {class:"panel",summary {"Whole-plan grocery totals"} div {class:"cost-table",table {thead {tr {th {"Ingredient"} th {"Need"} th {"Buy"} th {"Estimate"}}} tbody {for g in &p.groceries {tr {td {"{g.name}"} td {"{g.needed} {g.unit}"} td {"{g.packs} packs · {g.purchased} {g.unit}"} td {"{money(g.low_paise)}–{money(g.high_paise)}"}}}}}}}
                }
                div {class:"save-row",div {strong {if dirty{"Unsaved calendar changes"}else{"Calendar saved"}} p {if a.remote{"This plan has online storage. Save to keep draft changes and device progress."}else{"Offline drafts can be saved on this device."}}} button {class:"primary",disabled:session.busy||session.editing||!dirty,onclick:move|_|{
                    let Some(entry)=state().active else{return;};let endpoint=settings().endpoint;let online=connected();let saved=state().saved;state.write().busy=true;state.write().error.clear();
                    spawn(async move{
                        match persist_calendar(entry,saved,endpoint,online).await {
                            Ok((entry,entries,note))=>{let mut s=state.write();s.active=Some(entry);s.saved=entries;s.note=note;},
                            Err(e)=>state.write().error=e,
                        }
                        state.write().busy=false;
                    });
                },if session.busy{"Saving…"}else{"Save calendar →"}}}
                if !p.id.is_empty() {p {class:"plan-id","Plan ID: {p.id}"}}
                p {class:"hint","Nutrition and prices are authored demo estimates. The sampled search is not a proven minimum cost or an assessment of overall dietary adequacy."}
            }}
        } else {
            section {class:"planner-empty panel",span {class:"eyebrow","READY WHEN YOU ARE"} h3 {"Turn your targets into meals."} p {"Choose a start date and generate a calendar. Review every meal, swap dishes and shop from a list that accounts for the packs you have already bought."}}
        }
        details {class:"panel saved-calendars",summary {"Saved calendars ({session.saved.len()})"}
            p {class:"hint","The eight most recently saved calendars on this device. Opening one replaces the current draft; save your changes first."}
            div {class:"saved-plan-list",for (index,entry) in session.saved.iter().enumerate() {
                div {class:"saved-plan-row",
                    button {class:"secondary saved-plan-open",disabled:session.busy,onclick:move|_|{let entry=state().saved.get(index).cloned();state.write().active=entry;state.write().editing=false;copying.set(false);pending_delete.set(None);state.write().note="Saved calendar opened with its original targets and food catalogue.".into();state.write().error.clear();setup_open.set(false);week.set(0);calendar_page.set(0);selected_day.set(0);day_popup.set(false);swapping.set(None);tab.set("calendar".into());},"{entry.record.plan.request.start_date} · {entry.record.plan.request.days} days · {money(entry.record.plan.high_paise)}"}
                    {let entry=entry.clone();rsx!{button {class:"secondary",disabled:session.busy,onclick:move|_|edit_plan(entry.clone()),"Edit saved plan"}}}
                    {let entry=entry.clone();rsx!{button {class:"secondary danger",disabled:session.busy,onclick:move|_|{pending_delete.set(Some(entry.clone()));copying.set(false);spawn(async move{let _=document::eval("requestAnimationFrame(()=>document.getElementById('delete-plan-confirmation')?.scrollIntoView({block:'center'}));").await;});},"Delete saved plan"}}}
                }
            }}
            button {class:"secondary",disabled:session.busy||!connected(),onclick:move|_|{fetch_online(0);},"Browse online calendars"}
            if online_loaded() {
                div {class:"online-calendars saved-plan-list",
                    if online_page().items.is_empty() {p {class:"hint","No online calendars on this page."}}
                    for entry in online_page().items {
                        button {class:"secondary",disabled:session.busy,onclick:move|_|{open_online(entry.id.clone());},"Open {entry.start_date} · {entry.days} days"}
                    }
                    div {class:"restore-plan",
                        button {class:"secondary",disabled:session.busy||online_offset()==0,onclick:move|_|{fetch_online(online_offset().saturating_sub(20));},"Previous saved calendars"}
                        button {class:"secondary",disabled:session.busy||!online_page().has_more,onclick:move|_|{fetch_online(online_offset()+20);},"More saved calendars"}
                    }
                }
            }
            label {r#for:"restore-plan","Open a database plan by ID"} div {class:"restore-plan",input {id:"restore-plan",placeholder:"Saved plan ID",value:"{restore_id}",oninput:move|e|restore_id.set(e.value())} button {class:"secondary",disabled:session.busy||!connected()||restore_id().trim().is_empty(),onclick:move|_|{open_online(restore_id().trim().to_string());},"Open saved plan"}}
        }
    }
}
