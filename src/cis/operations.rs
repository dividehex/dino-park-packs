use chrono::SecondsFormat;
use chrono::Utc;
use cis_client::AsyncCisClientTrait;
use cis_client::CisClient;
use cis_profile::crypto::SecretStore;
use cis_profile::crypto::Signer;
use cis_profile::schema::AccessInformationProviderSubObject;
use cis_profile::schema::Display;
use cis_profile::schema::KeyValue;
use cis_profile::schema::Profile;
use cis_profile::schema::PublisherAuthority;
use failure::format_err;
use failure::Error;
use futures::future::IntoFuture;
use futures::Future;
use log::warn;
use std::collections::BTreeMap;
use std::sync::Arc;

fn insert_kv_and_sign_values_field(
    field: &mut AccessInformationProviderSubObject,
    kv: (String, Option<String>),
    store: &SecretStore,
    now: &str,
) -> Result<(), Error> {
    if let Some(KeyValue(ref mut values)) = &mut field.values {
        values.insert(kv.0, kv.1);
    } else {
        field.values = Some(KeyValue({
            let mut btm = BTreeMap::new();
            btm.insert(kv.0, kv.1);
            btm
        }))
    }
    if field.metadata.display.is_none() {
        field.metadata.display = Some(Display::Staff);
    }
    field.metadata.last_modified = now.to_owned();
    field.signature.publisher.name = PublisherAuthority::Mozilliansorg;
    store.sign_attribute(field)
}

fn remove_kv_and_sign_values_field(
    field: &mut AccessInformationProviderSubObject,
    k: &str,
    store: &SecretStore,
    now: &str,
) -> Result<(), Error> {
    if let Some(KeyValue(ref mut values)) = &mut field.values {
        if values.remove(k).is_some() {
            field.metadata.last_modified = now.to_owned();
            field.signature.publisher.name = PublisherAuthority::Mozilliansorg;
            return store.sign_attribute(field);
        }
    }
    warn!("group {} was not present when trying to delete", k);
    Ok(())
}

pub fn add_group_to_profile(
    cis_client: Arc<CisClient>,
    group_name: String,
    mut profile: Profile,
) -> Box<dyn Future<Item = (), Error = Error>> {
    let now = &Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    match insert_kv_and_sign_values_field(
        &mut profile.access_information.mozilliansorg,
        (group_name, None),
        cis_client.get_secret_store(),
        now,
    ) {
        Ok(_) => {
            if let Some(user_id) = profile.user_id.value.clone() {
                Box::new(cis_client.update_user(&user_id, profile).map(|_| ()))
            } else {
                Box::new(Err(format_err!("invalid user_id")).into_future())
            }
        }
        Err(e) => Box::new(Err(e).into_future()),
    }
}

pub fn remove_group_from_profile(
    cis_client: Arc<CisClient>,
    group_name: &str,
    mut profile: Profile,
) -> Box<dyn Future<Item = (), Error = Error>> {
    let now = &Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    match remove_kv_and_sign_values_field(
        &mut profile.access_information.mozilliansorg,
        group_name,
        cis_client.get_secret_store(),
        now,
    ) {
        Ok(_) => {
            if let Some(user_id) = profile.user_id.value.clone() {
                Box::new(cis_client.update_user(&user_id, profile).map(|_| ()))
            } else {
                Box::new(Err(format_err!("invalid user_id")).into_future())
            }
        }
        Err(e) => Box::new(Err(e).into_future()),
    }
}
