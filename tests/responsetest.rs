use chrono::prelude::*;
use http::*;
use http_cache_semantics::*;
use std::time::Duration;
use std::time::SystemTime;

macro_rules! headers(
    { $($key:tt : $value:expr),* $(,)? } => {
        {
            let mut m = Response::builder();
            $(
                m = m.header($key, $value);
            )+
            m.body(()).unwrap()
        }
     };
);

fn req() -> Request<()> {
    Request::get("http://test.example.com/").body(()).unwrap()
}

#[test]
fn simple_miss() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(&req(), &Response::new(()));
    assert!(cache.is_stale(now));
}

#[test]
fn simple_hit() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! { "cache-control": "public, max-age=999999" },
    );
    assert!(!cache.is_stale(now));
    assert_eq!(cache.time_to_live(now).as_secs(), 999999);
}

#[test]
fn weird_syntax() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! { "cache-control": ",,,,max-age =  456      ," },
    );
    assert!(!cache.is_stale(now));
    assert_eq!(cache.time_to_live(now).as_secs(), 456);

    // let cache2 = CachePolicy.fromObject(
    //     JSON.parse(JSON.stringify(cache.toObject()))
    // );
    // assert!(cache2 instanceof CachePolicy);
    // assert!(!cache2.is_stale(now));
    // assert_eq!(cache2.max_age().as_secs(), 456);
}

#[test]
fn quoted_syntax() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! { "cache-control": "  max-age = \"678\"      " },
    );
    assert!(!cache.is_stale(now));
    assert_eq!(cache.time_to_live(now).as_secs(), 678);
}

#[test]
fn iis() {
    let now = SystemTime::now();
    let cache = CachePolicy::new_options(
        &req().into_parts().0,
        &headers! {
            "cache-control": "private, public, max-age=259200"
        }
        .into_parts()
        .0,
        now,
        CacheOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert!(!cache.is_stale(now));
    assert_eq!(cache.time_to_live(now).as_secs(), 259200);
}

#[test]
fn pre_check_tolerated() {
    let now = SystemTime::now();
    let cc = "pre-check=0, post-check=0, no-store, no-cache, max-age=100";
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": cc
        },
    );
    assert!(cache.is_stale(now), "{:#?}", cache);
    assert!(!cache.is_storable());
    assert_eq!(cache.time_to_live(now).as_secs(), 0);
    assert_eq!(
        get_cached_response(
            &cache,
            &Request::get("http://test.example.com/")
                .header("cache-control", "max-stale")
                .body(())
                .unwrap(),
            now
        )
        .headers()["cache-control"],
        cc
    );
}

#[test]
fn pre_check_poison() {
    let now = SystemTime::now();
    let orig_cc = "pre-check=0, post-check=0, no-cache, no-store, max-age=100, custom, foo=bar";
    let res = &headers! { "cache-control": orig_cc, "pragma": "no-cache"};
    let cache = CachePolicy::new_options(
        &req(),
        res,
        now,
        CacheOptions {
            ignore_cargo_cult: true,
            ..Default::default()
        },
    );
    assert!(!cache.is_stale(now));
    assert!(cache.is_storable());
    assert_eq!(cache.time_to_live(now).as_secs(), 100);

    let cc = get_cached_response(&cache, &req(), now);
    let cc = cc.headers();
    let cc = cc["cache-control"].to_str().unwrap();
    assert!(!cc.contains("pre-check"));
    assert!(!cc.contains("post-check"));
    assert!(!cc.contains("no-store"));

    assert!(cc.contains("max-age=100"));
    assert!(cc.contains(", custom") || cc.contains("custom, "));
    assert!(cc.contains("foo=bar"));

    assert!(get_cached_response(
        &cache,
        &Request::get("http://test.example.com/")
            .header("cache-control", "max-stale")
            .body(())
            .unwrap(),
        now
    )
    .headers()
    .get("pragma")
    .is_none());
}

#[test]
fn pre_check_poison_undefined_header() {
    let now = SystemTime::now();
    let orig_cc = "pre-check=0, post-check=0, no-cache, no-store";
    let res = &headers! { "cache-control": orig_cc, "expires": "yesterday!"};
    let cache = CachePolicy::new_options(
        &req(),
        res,
        now,
        CacheOptions {
            ignore_cargo_cult: true,
            ..Default::default()
        },
    );
    assert!(cache.is_stale(now));
    assert!(cache.is_storable());
    assert_eq!(cache.time_to_live(now).as_secs(), 0);

    let res = &get_cached_response(
        &cache,
        &Request::get("http://test.example.com/")
            .header("cache-control", "max-stale")
            .body(())
            .unwrap(),
        now,
    );
    let _cc = &res.headers()["cache-control"];

    assert!(res.headers().get("expires").is_none());
}

#[test]
fn cache_with_expires() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "date": date_str(now),
            "expires": date_str(now + Duration::from_millis(2001)),
        },
    );
    assert!(!cache.is_stale(now));
    assert_eq!(2, cache.time_to_live(now).as_secs());
}

#[test]
fn cache_with_expires_relative_to_date() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "date": date_str(now - Duration::from_secs(30)),
            "expires": date_str(now),
        },
    );
    assert_eq!(30, cache.time_to_live(now).as_secs());
}

#[test]
fn cache_with_expires_always_relative_to_date() {
    let now = SystemTime::now();
    let cache = CachePolicy::new_options(
        &req(),
        &headers! {
            "date": date_str(now - Duration::from_secs(3)),
            "expires": date_str(now),
        },
        now,
        Default::default(),
    );
    assert_eq!(3, cache.time_to_live(now).as_secs());
}

#[test]
fn cache_expires_no_date() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": "public",
            "expires": date_str(now + Duration::from_secs(3600)),
        },
    );
    assert!(!cache.is_stale(now));
    assert!(cache.time_to_live(now).as_secs() > 3595);
    assert!(cache.time_to_live(now).as_secs() < 3605);
}

#[test]
fn ages() {
    let mut now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": "max-age=100",
            "age": "50",
        },
    );
    assert!(cache.is_storable());

    assert_eq!(50, cache.time_to_live(now).as_secs());
    assert!(!cache.is_stale(now));
    now += Duration::from_secs(48);
    assert_eq!(2, cache.time_to_live(now).as_secs());
    assert!(!cache.is_stale(now));
    now += Duration::from_secs(5);
    assert!(cache.is_stale(now));
    assert_eq!(0, cache.time_to_live(now).as_secs());
}

#[test]
fn age_can_make_stale() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": "max-age=100",
            "age": "101",
        },
    );
    assert!(cache.is_stale(now));
    assert!(cache.is_storable());
}

#[test]
fn age_not_always_stale() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": "max-age=20",
            "age": "15",
        },
    );
    assert!(!cache.is_stale(now));
    assert!(cache.is_storable());
}

#[test]
fn bogus_age_ignored() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": "max-age=20",
            "age": "golden",
        },
    );
    assert!(!cache.is_stale(now));
    assert!(cache.is_storable());
}

#[test]
fn cache_old_files() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "date": date_str(now),
            "last-modified": "Mon, 07 Mar 2016 11:52:56 GMT",
        },
    );
    assert!(!cache.is_stale(now));
    assert!(cache.time_to_live(now).as_secs() > 100);
}

#[test]
fn immutable_simple_hit() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! { "cache-control": "immutable, max-age=999999" },
    );
    assert!(!cache.is_stale(now));
    assert_eq!(cache.time_to_live(now).as_secs(), 999999);
}

#[test]
fn immutable_can_expire() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": "immutable, max-age=0"
        },
    );
    assert!(cache.is_stale(now));
    assert_eq!(cache.time_to_live(now).as_secs(), 0);
}

#[test]
fn cache_immutable_files() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "date": date_str(now),
            "cache-control": "immutable",
            "last-modified": date_str(now),
        },
    );
    assert!(!cache.is_stale(now));
    assert!(cache.time_to_live(now).as_secs() > 100);
}

#[test]
fn immutable_can_be_off() {
    let now = SystemTime::now();
    let cache = CachePolicy::new_options(
        &req(),
        &headers! {
            "date": date_str(now),
            "cache-control": "immutable",
            "last-modified": date_str(now),
        },
        now,
        CacheOptions {
            immutable_min_time_to_live: Duration::from_secs(0),
            ..Default::default()
        },
    );
    assert!(cache.is_stale(now));
    assert_eq!(cache.time_to_live(now).as_secs(), 0);
}

#[test]
fn pragma_no_cache() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "pragma": "no-cache",
            "last-modified": "Mon, 07 Mar 2016 11:52:56 GMT",
        },
    );
    assert!(cache.is_stale(now));
}

#[test]
fn blank_cache_control_and_pragma_no_cache() {
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": "",
            "pragma": "no-cache",
            "last-modified": date_str(SystemTime::now() - Duration::from_secs(10)),
        },
    );
    assert!(!cache.is_stale(SystemTime::now()), "{:#?}", cache);
}

#[test]
fn no_store() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! { "cache-control": "no-store, public, max-age=1", },
    );
    assert!(cache.is_stale(now));
    assert_eq!(0, cache.time_to_live(now).as_secs());
}

#[test]
fn observe_private_cache() {
    let now = SystemTime::now();
    let proxy_cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": "private, max-age=1234",
        },
    );
    assert!(proxy_cache.is_stale(now));
    assert_eq!(0, proxy_cache.time_to_live(now).as_secs());

    let ua_cache = CachePolicy::new_options(
        &req(),
        &headers! {
            "cache-control": "private, max-age=1234",
        },
        now,
        CacheOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert!(!ua_cache.is_stale(now));
    assert_eq!(1234, ua_cache.time_to_live(now).as_secs());
}

#[test]
fn don_t_share_cookies() {
    let now = SystemTime::now();
    let proxy_cache = CachePolicy::new_options(
        &req(),
        &headers! {
            "set-cookie": "foo=bar",
            "cache-control": "max-age=99",
        },
        now,
        CacheOptions {
            shared: true,
            ..Default::default()
        },
    );
    assert!(proxy_cache.is_stale(now));
    assert_eq!(0, proxy_cache.time_to_live(now).as_secs());

    let ua_cache = CachePolicy::new_options(
        &req(),
        &headers! {
            "set-cookie": "foo=bar",
            "cache-control": "max-age=99",
        },
        now,
        CacheOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert!(!ua_cache.is_stale(now));
    assert_eq!(99, ua_cache.time_to_live(now).as_secs());
}

#[test]
fn do_share_cookies_if_immutable() {
    let now = SystemTime::now();
    let proxy_cache = CachePolicy::new_options(
        &req(),
        &headers! {
            "set-cookie": "foo=bar",
            "cache-control": "immutable, max-age=99",
        },
        now,
        CacheOptions {
            shared: true,
            ..Default::default()
        },
    );
    assert!(!proxy_cache.is_stale(now));
    assert_eq!(99, proxy_cache.time_to_live(now).as_secs());
}

#[test]
fn cache_explicitly_public_cookie() {
    let now = SystemTime::now();
    let proxy_cache = CachePolicy::new_options(
        &req(),
        &headers! {
            "set-cookie": "foo=bar",
            "cache-control": "max-age=5, public",
        },
        now,
        CacheOptions {
            shared: true,
            ..Default::default()
        },
    );
    assert!(!proxy_cache.is_stale(now));
    assert_eq!(5, proxy_cache.time_to_live(now).as_secs());
}

#[test]
fn miss_max_age_0() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! { "cache-control": "public, max-age=0",       },
    );
    assert!(cache.is_stale(now));
    assert_eq!(0, cache.time_to_live(now).as_secs());
}

#[test]
fn uncacheable_503() {
    let now = SystemTime::now();
    let mut res = headers! {               "cache-control": "public, max-age=1000" };
    *res.status_mut() = StatusCode::from_u16(503).unwrap();
    let cache = CachePolicy::new(&req(), &res);
    assert!(cache.is_stale(now));
    assert_eq!(0, cache.time_to_live(now).as_secs());
}

#[test]
fn cacheable_301() {
    let now = SystemTime::now();
    let mut res = headers! { "last-modified": "Mon, 07 Mar 2016 11:52:56 GMT", };
    *res.status_mut() = StatusCode::from_u16(301).unwrap();
    let cache = CachePolicy::new(&req(), &res);
    assert!(!cache.is_stale(now));
}

#[test]
fn uncacheable_303() {
    let now = SystemTime::now();
    let mut res = headers! { "last-modified": "Mon, 07 Mar 2016 11:52:56 GMT", };
    *res.status_mut() = StatusCode::from_u16(303).unwrap();
    let cache = CachePolicy::new(&req(), &res);
    assert!(cache.is_stale(now));
    assert_eq!(0, cache.time_to_live(now).as_secs());
}

#[test]
fn cacheable_303() {
    let now = SystemTime::now();
    let mut res = headers! { "cache-control": "max-age=1000", };
    *res.status_mut() = StatusCode::from_u16(303).unwrap();
    let cache = CachePolicy::new(&req(), &res);
    assert!(!cache.is_stale(now));
}

#[test]
fn uncacheable_412() {
    let now = SystemTime::now();
    let mut res = headers! { "cache-control": "public, max-age=1000", };
    *res.status_mut() = StatusCode::from_u16(412).unwrap();
    let cache = CachePolicy::new(&req(), &res);
    assert!(cache.is_stale(now));
    assert_eq!(0, cache.time_to_live(now).as_secs());
}

#[test]
fn expired_expires_cached_with_max_age() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": "public, max-age=9999",
            "expires": "Sat, 07 May 2016 15:35:18 GMT",
        },
    );
    assert!(!cache.is_stale(now));
    assert_eq!(9999, cache.time_to_live(now).as_secs());
}

#[test]
fn expired_expires_cached_with_s_maxage() {
    let now = SystemTime::now();
    let proxy_cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": "public, s-maxage=9999",
            "expires": "Sat, 07 May 2016 15:35:18 GMT",
        },
    );
    assert!(!proxy_cache.is_stale(now));
    assert_eq!(9999, proxy_cache.time_to_live(now).as_secs());

    let ua_cache = CachePolicy::new_options(
        &req(),
        &headers! {
            "cache-control": "public, s-maxage=9999",
            "expires": "Sat, 07 May 2016 15:35:18 GMT",
        },
        now,
        CacheOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert!(ua_cache.is_stale(now));
    assert_eq!(0, ua_cache.time_to_live(now).as_secs());
}

#[test]
fn max_age_wins_over_future_expires() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": "public, max-age=333",
            "expires": date_str(now + Duration::from_secs(3600)),
        },
    );
    assert!(!cache.is_stale(now));
    assert_eq!(333, cache.time_to_live(now).as_secs());
}

#[test]
fn remove_hop_headers() {
    let mut now = SystemTime::now();
    let res = &headers! {
        "te": "deflate",
        "date": "now",
        "custom": "header",
        "oompa": "lumpa",
        "connection": "close, oompa, header",
        "age": "10",
        "cache-control": "public, max-age=333",
    };
    let cache = CachePolicy::new(&req(), res);

    now += Duration::from_millis(1005);
    let h = get_cached_response(&cache, &req(), now);
    let h = h.headers();
    assert!(h.get("connection").is_none());
    assert!(h.get("te").is_none());
    assert!(h.get("oompa").is_none());
    assert_eq!(h["cache-control"].to_str().unwrap(), "public, max-age=333");
    assert_ne!(
        h["date"].to_str().unwrap(),
        "now",
        "updated age requires updated date"
    );
    assert_eq!(h["custom"].to_str().unwrap(), "header");
    assert_eq!(h["age"].to_str().unwrap(), "11");

    // let cache2 = TimeTravellingPolicy.fromObject(
    //     JSON.parse(JSON.stringify(cache.toObject()))
    // );
    // assert!(cache2 instanceof TimeTravellingPolicy);
    // let h2 = cache2.cached_response(now).headers();
    // assert.deepEqual(h, h2);
}

fn date_str(now: SystemTime) -> String {
    let timestamp = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let date = DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(timestamp as _, 0), Utc);
    date.to_rfc2822()
}

fn get_cached_response(
    policy: &CachePolicy,
    req: &impl http_cache_semantics::RequestLike,
    now: SystemTime,
) -> http::response::Parts {
    match policy.before_request(req, now) {
        http_cache_semantics::BeforeRequest::Fresh(res) => res,
        _ => panic!("stale"),
    }
}
