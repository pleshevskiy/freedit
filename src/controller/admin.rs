use super::{
    get_site_config, into_response, timestamp_to_date, u8_slice_to_u32, Claim, IterType, PageData,
    SiteConfig, ValidatedForm,
};
use crate::{
    controller::{ivec_to_u32, Comment, Inn, Post, Solo, User},
    error::AppError,
};
use askama::Template;
use axum::{
    extract::{Query, State, TypedHeader},
    headers::Cookie,
    response::{IntoResponse, Redirect},
};
use bincode::config::standard;
use serde::Deserialize;
use sled::Db;

/// Page data: `admin_view.html`
#[derive(Template)]
#[template(path = "admin_view.html", escape = "none")]
struct PageAdminView<'a> {
    page_data: PageData<'a>,
    tree_names: Vec<String>,
    ones: Vec<String>,
    tree_name: String,
    anchor: usize,
    is_desc: bool,
    n: usize,
}

/// url params: `admin_view.html`
#[derive(Deserialize)]
pub(crate) struct ParamsAdminView {
    tree_name: Option<String>,
    anchor: Option<usize>,
    is_desc: Option<bool>,
}

/// `GET /admin/view`
pub(crate) async fn admin_view(
    State(db): State<Db>,
    cookie: Option<TypedHeader<Cookie>>,
    Query(params): Query<ParamsAdminView>,
) -> Result<impl IntoResponse, AppError> {
    let cookie = cookie.ok_or(AppError::NonLogin)?;
    let site_config = get_site_config(&db)?;
    let claim = Claim::get(&db, &cookie, &site_config).ok_or(AppError::NonLogin)?;
    if claim.role != u8::MAX {
        return Err(AppError::Unauthorized);
    }

    let n = site_config.per_page;
    let anchor = params.anchor.unwrap_or(0);
    let is_desc = params.is_desc.unwrap_or(true);

    let mut tree_names = Vec::with_capacity(64);
    for i in db.tree_names() {
        let name = String::from_utf8_lossy(&i);
        tree_names.push(name.to_string());
    }
    tree_names.sort_unstable();

    let mut ones = Vec::with_capacity(n);
    let tree_name = params
        .tree_name
        .unwrap_or_else(|| "__sled__default".to_owned());

    if tree_names.contains(&tree_name) {
        let tree = db.open_tree(&tree_name)?;
        let iter = if is_desc {
            IterType::Rev(tree.iter().rev())
        } else {
            IterType::Iter(tree.iter())
        };

        for (idx, i) in iter.enumerate() {
            if idx < anchor {
                continue;
            }

            if idx >= anchor + n {
                break;
            }

            let (k, v) = i?;
            match tree_name.as_str() {
                "__sled__default" => {
                    let key = String::from_utf8_lossy(&k);
                    if key == "site_config" {
                        let (site_config, _): (SiteConfig, usize) =
                            bincode::decode_from_slice(&v, standard()).unwrap_or_default();
                        ones.push(format!("{}: {:?}", key, site_config));
                    } else {
                        let v = ivec_to_u32(&v);
                        ones.push(format!("{}: {}", key, v));
                    };
                }
                "users" => {
                    let key = ivec_to_u32(&k);
                    let (mut one, _): (User, usize) = bincode::decode_from_slice(&v, standard())?;
                    one.password_hash = String::from("******");
                    one.recovery_hash = None;
                    ones.push(format!("{}: {:?}", key, one));
                }
                "solos" => {
                    let key = ivec_to_u32(&k);
                    let (one, _): (Solo, usize) = bincode::decode_from_slice(&v, standard())?;
                    ones.push(format!("{}: {:?}", key, one));
                }
                "inns" => {
                    let key = ivec_to_u32(&k);
                    let (one, _): (Inn, usize) = bincode::decode_from_slice(&v, standard())?;
                    ones.push(format!("{}: {:?}", key, one));
                }
                "posts" => {
                    let key = ivec_to_u32(&k);
                    let (one, _): (Post, usize) = bincode::decode_from_slice(&v, standard())?;
                    ones.push(format!("{}: {:?}", key, one));
                }
                "post_comments" => {
                    let pid = u8_slice_to_u32(&k[0..4]);
                    let cid = u8_slice_to_u32(&k[4..8]);
                    let (one, _): (Comment, usize) = bincode::decode_from_slice(&v, standard())?;
                    ones.push(format!("pid: {}, cid: {}, comment: {:?}", pid, cid, one));
                }
                "user_comments" => {
                    let uid = u8_slice_to_u32(&k[0..4]);
                    let pid = u8_slice_to_u32(&k[4..8]);
                    let cid = u8_slice_to_u32(&k[8..12]);
                    ones.push(format!("uid: {}, pid: {}, cid: {}", uid, pid, cid));
                }
                "comment_upvotes" | "comment_downvotes" => {
                    let pid = u8_slice_to_u32(&k[0..4]);
                    let cid = u8_slice_to_u32(&k[4..8]);
                    let uid = u8_slice_to_u32(&k[8..12]);
                    ones.push(format!("pid: {}, cid: {}, uid: {}", pid, cid, uid));
                }
                "post_timeline_idx" => {
                    let id = u8_slice_to_u32(&k[0..4]);
                    let idx = u8_slice_to_u32(&k[4..8]);
                    let v = ivec_to_u32(&v);
                    ones.push(format!("id: {}, idx: {}, target: {}", id, idx, v));
                }
                "user_posts" => {
                    let uid = u8_slice_to_u32(&k[0..4]);
                    let pid = u8_slice_to_u32(&k[4..8]);
                    let iid = u8_slice_to_u32(&v[0..4]);
                    let visibility = u8_slice_to_u32(&v[4..8]);
                    ones.push(format!(
                        "uid: {},  iid: {}, pid: {}, visibility: {}",
                        uid, iid, pid, visibility
                    ));
                }
                "post_comments_count" | "post_pageviews" => {
                    let id = u8_slice_to_u32(&k);
                    let count = ivec_to_u32(&v);
                    ones.push(format!("id: {}, count: {}", id, count));
                }
                "hashtags" | "topics" | "tags" => {
                    let len = k.len();
                    let str = String::from_utf8_lossy(&k[0..len - 4]);
                    let id = u8_slice_to_u32(&k[len - 4..]);
                    ones.push(format!("{}#{}", str, id));
                }
                "user_following" | "user_followers" | "mod_inns" | "user_inns" | "inn_users"
                | "inn_apply" | "post_upvotes" | "post_downvotes" | "user_solos_like"
                | "inn_posts" | "solo_users_like" => {
                    let id1 = u8_slice_to_u32(&k[0..4]);
                    let id2 = u8_slice_to_u32(&k[4..8]);
                    ones.push(format!("k: {}#{}, v: {:?}", id1, id2, v));
                }
                "user_stats" => {
                    let mut k_str = std::str::from_utf8(&k)?.split('_');
                    let timestamp = i64::from_str_radix(k_str.next().unwrap(), 16).unwrap();
                    let date = timestamp_to_date(timestamp);
                    let uid = k_str.next().unwrap();
                    let stat_type = k_str.next().unwrap().to_owned();
                    let count = ivec_to_u32(&v);
                    ones.push(format!("{} - {} - {} - {}", uid, date, stat_type, count));
                }
                "inn_names" | "usernames" => {
                    let name = std::str::from_utf8(&k)?;
                    let id = u8_slice_to_u32(&v);
                    ones.push(format!("name: {}, id: {}", name, id));
                }
                "static_user_post" | "static_inn_post" | "inns_private" => {
                    let id = u8_slice_to_u32(&k);
                    ones.push(format!("id: {}", id));
                }
                "user_solos" => {
                    let uid = u8_slice_to_u32(&k[0..4]);
                    let sid = u8_slice_to_u32(&k[4..8]);
                    let visibility = u8_slice_to_u32(&v);
                    ones.push(format!(
                        "uid: {}, sid: {}, visibility: {}",
                        uid, sid, visibility
                    ));
                }
                "solo_timeline" => {
                    let sid = u8_slice_to_u32(&k[0..4]);
                    let uid = u8_slice_to_u32(&v[0..4]);
                    let visibility = u8_slice_to_u32(&v[4..8]);
                    ones.push(format!(
                        "sid: {}, uid: {}, visibility: {}",
                        sid, uid, visibility
                    ));
                }
                "notifications" => {
                    let pid = u8_slice_to_u32(&k[0..4]);
                    let cid = u8_slice_to_u32(&k[4..8]);
                    let uid = u8_slice_to_u32(&k[8..12]);
                    ones.push(format!(
                        "uid: {}, pid: {}, cid: {}, notification_code:{}",
                        pid, cid, uid, v[0]
                    ));
                }
                "captcha" | "sessions" => {
                    let k_str = std::str::from_utf8(&k)?;
                    let time_stamp = k_str
                        .split_once('_')
                        .and_then(|s| i64::from_str_radix(s.0, 16).ok())
                        .unwrap();
                    ones.push(format!("timestamp: {}", time_stamp));
                }
                "post_timeline" => {
                    let timestamp = i64::from(u8_slice_to_u32(&k[0..4]));
                    let date = timestamp_to_date(timestamp);
                    let iid = u8_slice_to_u32(&k[4..8]);
                    let pid = u8_slice_to_u32(&k[8..12]);
                    let visibility = u8_slice_to_u32(&v);
                    ones.push(format!("{} - {} - {} - {}", date, iid, pid, visibility));
                }
                "user_uploads" => {
                    let uid = u8_slice_to_u32(&k[0..4]);
                    let img = String::from_utf8_lossy(&k[4..]);
                    ones.push(format!("{} - {}", uid, img));
                }
                _ => ones.push(format!("{} has not been supported yet", tree_name)),
            }
        }
    }

    let page_data = PageData::new("Admin view", &site_config, Some(claim), false);
    let page_admin_view = PageAdminView {
        page_data,
        tree_names,
        ones,
        tree_name,
        anchor,
        is_desc,
        n,
    };
    Ok(into_response(&page_admin_view, "html"))
}

/// Page data: `admin.html`
#[derive(Template)]
#[template(path = "admin.html")]
struct PageAdmin<'a> {
    site_config: &'a SiteConfig,
    page_data: PageData<'a>,
}

/// `GET /admin/site_setting`
pub(crate) async fn admin(
    State(db): State<Db>,
    cookie: Option<TypedHeader<Cookie>>,
) -> Result<impl IntoResponse, AppError> {
    let cookie = cookie.ok_or(AppError::NonLogin)?;
    let site_config = get_site_config(&db)?;
    let claim = Claim::get(&db, &cookie, &site_config).ok_or(AppError::NonLogin)?;
    if claim.role != u8::MAX {
        return Err(AppError::Unauthorized);
    }

    let page_data = PageData::new("Admin", &site_config, Some(claim), false);
    let page_admin = PageAdmin {
        site_config: &site_config,
        page_data,
    };
    Ok(into_response(&page_admin, "html"))
}

/// `POST /admin`
pub(crate) async fn admin_post(
    State(db): State<Db>,
    cookie: Option<TypedHeader<Cookie>>,
    ValidatedForm(input): ValidatedForm<SiteConfig>,
) -> Result<impl IntoResponse, AppError> {
    let cookie = cookie.ok_or(AppError::NonLogin)?;
    let claim = Claim::get(&db, &cookie, &input).ok_or(AppError::NonLogin)?;
    if claim.role != u8::MAX {
        return Err(AppError::Unauthorized);
    }

    let site_config = bincode::encode_to_vec(&input, standard())?;
    db.insert("site_config", site_config)?;
    Ok(Redirect::to("/admin"))
}

impl Default for SiteConfig {
    fn default() -> Self {
        SiteConfig {
            site_name: "freedit".into(),
            domain: "http://127.0.0.1:3001".into(),
            description: "a forum powered by rust".into(),
            read_only: false,
            inn_mod_max: 5,
            title_max_length: 100,
            article_max_length: 65535,
            comment_max_length: 10_000,
            solo_interval: 10,
            post_interval: 30,
            comment_interval: 10,
            per_page: 30,
            static_page: 10,
            captcha_difficulty: "Easy".into(),
            captcha_name: "Amelia".into(),
        }
    }
}

/// Page data: `admin_stats.html`
#[derive(Template)]
#[template(path = "admin_stats.html")]
struct PageAdminStats<'a> {
    page_data: PageData<'a>,
    stats: Vec<(String, String, String, u32)>,
}

/// `GET /admin/stats`
pub(crate) async fn admin_stats(
    State(db): State<Db>,
    cookie: Option<TypedHeader<Cookie>>,
) -> Result<impl IntoResponse, AppError> {
    let cookie = cookie.ok_or(AppError::NonLogin)?;
    let site_config = get_site_config(&db)?;
    let claim = Claim::get(&db, &cookie, &site_config).ok_or(AppError::NonLogin)?;
    if claim.role != u8::MAX {
        return Err(AppError::Unauthorized);
    }

    let mut stats = Vec::with_capacity(100);
    for i in &db.open_tree("user_stats")? {
        let (k, v) = i?;
        let mut k_str = std::str::from_utf8(&k)?.split('_');
        let timestamp = i64::from_str_radix(k_str.next().unwrap(), 16).unwrap();
        let date = timestamp_to_date(timestamp);
        let uid = k_str.next().unwrap().to_owned();
        let stat_type = k_str.next().unwrap().to_owned();
        let count = ivec_to_u32(&v);
        stats.push((uid, date, stat_type, count));
    }

    stats.sort_unstable_by(|a, b| b.3.cmp(&a.3));
    if stats.len() > 100 {
        stats.truncate(100);
    }

    let page_data = PageData::new("Admin-pageview", &site_config, Some(claim), false);
    let page_admin_pageview = PageAdminStats { page_data, stats };
    Ok(into_response(&page_admin_pageview, "html"))
}
