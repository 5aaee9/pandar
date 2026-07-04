pub(super) enum LocalRoute {
    SignIn,
    AppJs,
    StylesCss,
    GetConfig,
    PostConfig,
    Callback,
    BadRequest,
    NotFound,
}

pub(super) fn classify(method: &str, path: &str) -> LocalRoute {
    if path.split('/').any(|segment| segment == "..") {
        return LocalRoute::BadRequest;
    }

    match (method, path) {
        ("GET", "/sign-in") => LocalRoute::SignIn,
        ("GET", "/assets/app.js") => LocalRoute::AppJs,
        ("GET", "/assets/styles.css") => LocalRoute::StylesCss,
        ("GET", "/config") => LocalRoute::GetConfig,
        ("POST", "/config") => LocalRoute::PostConfig,
        ("GET", "/callback") => LocalRoute::Callback,
        _ if method == "GET" && is_localized_sign_in(path) => LocalRoute::SignIn,
        _ => LocalRoute::NotFound,
    }
}

fn is_localized_sign_in(path: &str) -> bool {
    let Some(locale) = path
        .strip_prefix('/')
        .and_then(|path| path.strip_suffix("/sign-in"))
    else {
        return false;
    };
    is_locale(locale)
}

fn is_locale(value: &str) -> bool {
    let Some((language, region)) = value.split_once(['-', '_']) else {
        return matches!(value.len(), 2 | 3)
            && value.bytes().all(|byte| byte.is_ascii_alphabetic());
    };
    matches!(language.len(), 2 | 3)
        && language.bytes().all(|byte| byte.is_ascii_alphabetic())
        && region.len() == 2
        && region.bytes().all(|byte| byte.is_ascii_alphabetic())
}
