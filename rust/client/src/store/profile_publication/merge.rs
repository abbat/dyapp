//! Post-prepare local edits win; constrained values and directory pairs remain indivisible.
use dyapp_profile::Profile;

fn changed<T: Clone + PartialEq>(base: &T, local: &T, remote: &T) -> T {
    if local == base {
        remote.clone()
    } else {
        local.clone()
    }
}

macro_rules! value {
    ($base:ident, $local:ident, $remote:ident, $field:ident) => {
        changed(&$base.$field, &$local.$field, &$remote.$field)
    };
}

// Exhaustive construction deliberately makes every new wire field require an overlay decision.
pub(super) fn merge(base: &Profile, local: &Profile, remote: &Profile) -> Profile {
    Profile {
        version: remote.version,
        deleted: remote.deleted,
        previous: remote.previous.clone(),
        search_only: remote.search_only,
        country: location(base, local, remote).country.clone(),
        income_from: income(base, local, remote).income_from,
        income_to: income(base, local, remote).income_to,
        has_kids: value!(base, local, remote, has_kids),
        wants_more_kids: value!(base, local, remote, wants_more_kids),
        interests: value!(base, local, remote, interests),
        place: location(base, local, remote).place.clone(),
        photos: value!(base, local, remote, photos),
        pow_nonce: proof(base, local, remote).pow_nonce,
        pow_bits: proof(base, local, remote).pow_bits,
        devices: directory(base, local, remote).devices.clone(),
        name: value!(base, local, remote, name),
        education_level: value!(base, local, remote, education_level),
        profession: value!(base, local, remote, profession),
        company: value!(base, local, remote, company),
        industry: value!(base, local, remote, industry),
        career_stage: value!(base, local, remote, career_stage),
        work_life_balance: value!(base, local, remote, work_life_balance),
        birth_year: birth(base, local, remote).birth_year,
        birth_month: birth(base, local, remote).birth_month,
        birth_day: birth(base, local, remote).birth_day,
        gender: value!(base, local, remote, gender),
        orientation: value!(base, local, remote, orientation),
        relationship_goal: value!(base, local, remote, relationship_goal),
        role_preference: value!(base, local, remote, role_preference),
        career_ambition: value!(base, local, remote, career_ambition),
        financial_philosophy: value!(base, local, remote, financial_philosophy),
        employment_status: value!(base, local, remote, employment_status),
        fitness_level: value!(base, local, remote, fitness_level),
        zodiac: value!(base, local, remote, zodiac),
        device_owners: directory(base, local, remote).device_owners.clone(),
        height: value!(base, local, remote, height),
        about: value!(base, local, remote, about),
        smoking: value!(base, local, remote, smoking),
        drinking: value!(base, local, remote, drinking),
        tattoos: value!(base, local, remote, tattoos),
        marital_status: value!(base, local, remote, marital_status),
        pets: value!(base, local, remote, pets),
        seeking_genders: seeking(base, local, remote).seeking_genders.clone(),
        seeking_age_from: seeking(base, local, remote).seeking_age_from,
        seeking_age_to: seeking(base, local, remote).seeking_age_to,
        not_looking: seeking(base, local, remote).not_looking,
    }
}

// Directory consent never publishes or discards pre-existing editable draft values.
// Directory changes made after preparation still use the real local baseline.
pub(super) fn directory_completion(base: &Profile, local: &Profile, remote: &Profile) -> Profile {
    let mut comparison = remote.clone();
    comparison.devices.clone_from(&base.devices);
    comparison.device_owners.clone_from(&base.device_owners);
    merge(&comparison, local, remote)
}

fn location<'a>(base: &Profile, local: &'a Profile, remote: &'a Profile) -> &'a Profile {
    if (&base.country, &base.place) == (&local.country, &local.place) {
        remote
    } else {
        local
    }
}

fn income<'a>(base: &Profile, local: &'a Profile, remote: &'a Profile) -> &'a Profile {
    if (base.income_from, base.income_to) == (local.income_from, local.income_to) {
        remote
    } else {
        local
    }
}

fn birth<'a>(base: &Profile, local: &'a Profile, remote: &'a Profile) -> &'a Profile {
    if (base.birth_year, base.birth_month, base.birth_day)
        == (local.birth_year, local.birth_month, local.birth_day)
    {
        remote
    } else {
        local
    }
}

fn seeking<'a>(base: &Profile, local: &'a Profile, remote: &'a Profile) -> &'a Profile {
    if (
        base.not_looking,
        &base.seeking_genders,
        base.seeking_age_from,
        base.seeking_age_to,
    ) == (
        local.not_looking,
        &local.seeking_genders,
        local.seeking_age_from,
        local.seeking_age_to,
    ) {
        remote
    } else {
        local
    }
}

fn directory<'a>(base: &Profile, local: &'a Profile, remote: &'a Profile) -> &'a Profile {
    if (&base.devices, &base.device_owners) == (&local.devices, &local.device_owners) {
        remote
    } else {
        local
    }
}

fn proof<'a>(base: &Profile, local: &'a Profile, remote: &'a Profile) -> &'a Profile {
    let changed = (base.pow_nonce, base.pow_bits) != (local.pow_nonce, local.pow_bits);
    if local.pow_bits > remote.pow_bits || (changed && local.pow_bits == remote.pow_bits) {
        local
    } else {
        remote
    }
}
