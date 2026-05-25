use crate::provider_store_types::{
    AuthenticatedBehavior, EffectiveRoute, ProviderProfile, ProviderProfileMode,
    ProviderProfileSaveRequest, ProviderProfilesState,
};

pub(crate) fn sanitize_provider_profile(
    request: ProviderProfileSaveRequest,
) -> Result<ProviderProfile, String> {
    let id = request
        .id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("profile-{}", crate::now_nanos()));
    let name = request.name.trim().to_string();
    let base_url = request.base_url.trim().to_string();
    let bearer_token = request.bearer_token.trim().to_string();
    let mode = request.mode;
    let upstream_protocol = request.upstream_protocol;
    let authenticated_behavior = request.authenticated_behavior;
    if name.is_empty() {
        return Err("配置档名称不能为空。".to_string());
    }
    if base_url.is_empty() {
        return Err("Base URL 不能为空。".to_string());
    }
    if bearer_token.is_empty() {
        return Err("API Key 不能为空。".to_string());
    }
    Ok(ProviderProfile {
        id,
        name,
        base_url,
        bearer_token,
        mode,
        upstream_protocol,
        authenticated_behavior,
    })
}

pub(crate) fn sanitize_provider_profiles_state(
    mut state: ProviderProfilesState,
) -> Result<ProviderProfilesState, String> {
    state.profiles = state
        .profiles
        .into_iter()
        .map(|profile| ProviderProfile {
            id: profile.id.trim().to_string(),
            name: profile.name.trim().to_string(),
            base_url: profile.base_url.trim().to_string(),
            bearer_token: profile.bearer_token.trim().to_string(),
            mode: profile.mode,
            upstream_protocol: profile.upstream_protocol,
            authenticated_behavior: profile.authenticated_behavior,
        })
        .filter(|profile| !profile.id.is_empty() && !profile.name.is_empty())
        .collect();
    if state.profiles.is_empty() {
        state = ProviderProfilesState::default();
    }
    if !state
        .profiles
        .iter()
        .any(|profile| profile.id == state.active_profile_id)
    {
        state.active_profile_id = state.profiles[0].id.clone();
    }
    Ok(state)
}

pub(crate) fn infer_effective_route(
    provider: &codex_pilot_core::relay_config::RelayProviderConfig,
    active_profile: Option<&ProviderProfile>,
    official_snapshot_available: bool,
) -> EffectiveRoute {
    if !provider.active {
        return EffectiveRoute::OfficialDirect;
    }
    if provider.authenticated {
        if active_profile
            .map(|profile| profile.authenticated_behavior == AuthenticatedBehavior::OfficialDirect)
            .unwrap_or(false)
            && !official_snapshot_available
        {
            return EffectiveRoute::DegradedRelay;
        }
        return EffectiveRoute::RelayAuthenticated;
    }
    if active_profile
        .map(|profile| profile.authenticated_behavior == AuthenticatedBehavior::OfficialDirect)
        .unwrap_or(false)
    {
        return EffectiveRoute::DegradedRelay;
    }
    EffectiveRoute::RelayApi
}

pub(crate) fn provider_status_message(
    provider: &codex_pilot_core::relay_config::RelayProviderConfig,
    active_profile: Option<&ProviderProfile>,
    official_snapshot_available: bool,
    route: EffectiveRoute,
) -> String {
    match route {
        EffectiveRoute::OfficialDirect => "当前使用官方原版配置。".to_string(),
        EffectiveRoute::RelayAuthenticated => "当前按登录态使用自动中转。".to_string(),
        EffectiveRoute::RelayApi => "当前按 API 形态使用自动中转。".to_string(),
        EffectiveRoute::DegradedRelay => {
            if !provider.authenticated
                && active_profile
                    .map(|profile| {
                        profile.authenticated_behavior == AuthenticatedBehavior::OfficialDirect
                    })
                    .unwrap_or(false)
            {
                "未检测到官方登录，当前已按 API 中转应用。".to_string()
            } else if !official_snapshot_available {
                "未找到官方原版快照，已退化为自动中转。".to_string()
            } else {
                "当前已退化为自动中转。".to_string()
            }
        }
    }
}

pub(crate) fn profiles_equivalent(
    profile: &ProviderProfile,
    candidate: &codex_pilot_core::ccs_import::CcsProviderCandidate,
    mode: ProviderProfileMode,
) -> bool {
    profile.mode == mode
        && profile.upstream_protocol == candidate.upstream_protocol
        && normalize_compare_text(&profile.name) == normalize_compare_text(&candidate.name)
        && normalize_base_url(&profile.base_url) == normalize_base_url(&candidate.base_url)
}

pub(crate) fn unique_imported_profile_name(
    existing: &[ProviderProfile],
    original_name: &str,
) -> String {
    let base = original_name.trim();
    if base.is_empty() {
        return unique_imported_profile_name(existing, "CCS 配置");
    }
    if !existing
        .iter()
        .any(|profile| normalize_compare_text(&profile.name) == normalize_compare_text(base))
    {
        return base.to_string();
    }

    let first_candidate = format!("{base} (CCS)");
    if !existing.iter().any(|profile| {
        normalize_compare_text(&profile.name) == normalize_compare_text(&first_candidate)
    }) {
        return first_candidate;
    }

    let mut index = 2usize;
    loop {
        let candidate = format!("{base} (CCS {index})");
        if !existing.iter().any(|profile| {
            normalize_compare_text(&profile.name) == normalize_compare_text(&candidate)
        }) {
            return candidate;
        }
        index += 1;
    }
}

pub(crate) fn unique_profile_id(existing: &[ProviderProfile]) -> String {
    let mut next_id = format!("profile-{}", crate::now_nanos());
    while existing.iter().any(|profile| profile.id == next_id) {
        next_id = format!("profile-{}", crate::now_nanos());
    }
    next_id
}

fn normalize_compare_text(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_base_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_ascii_lowercase()
}
