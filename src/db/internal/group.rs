use crate::db::internal;
use crate::db::logs::log_comment_body;
use crate::db::logs::LogContext;
use crate::db::model::*;
use crate::db::operations::models::GroupUpdate;
use crate::db::operations::models::GroupWithTermsFlag;
use crate::db::operations::models::NewGroup;
use crate::db::schema;
use crate::db::types::*;

use diesel::dsl::exists;
use diesel::dsl::select;
use diesel::prelude::*;
use failure::Error;
use serde_json::Value;
use uuid::Uuid;

pub fn get_group_with_terms_flag(
    connection: &PgConnection,
    group_name: &str,
) -> Result<GroupWithTermsFlag, Error> {
    let group = schema::groups::table
        .filter(schema::groups::name.eq(group_name))
        .filter(schema::groups::active.eq(true))
        .first::<Group>(connection)?;
    let terms = select(exists(
        schema::terms::table.filter(schema::terms::group_id.eq(group.id)),
    ))
    .get_result(connection)?;
    Ok(GroupWithTermsFlag { group, terms })
}

pub fn get_group(connection: &PgConnection, group_name: &str) -> Result<Group, Error> {
    schema::groups::table
        .filter(schema::groups::name.eq(group_name))
        .filter(schema::groups::active.eq(true))
        .first::<Group>(connection)
        .map_err(Into::into)
}

pub fn get_groups_by_ids(
    connection: &PgConnection,
    group_ids: &[i32],
) -> Result<Vec<Group>, Error> {
    schema::groups::table
        .filter(schema::groups::group_id.eq_any(group_ids))
        .filter(schema::groups::active.eq(true))
        .get_results::<Group>(connection)
        .map_err(Into::into)
}

pub fn add_group(
    host_uuid: &Uuid,
    connection: &PgConnection,
    new_group: NewGroup,
) -> Result<Group, Error> {
    let group = InsertGroup {
        name: new_group.name,
        active: true,
        path: String::from("/access_information/mozillians/"),
        description: new_group.description,
        capabilities: new_group.capabilities,
        typ: new_group.typ,
        trust: new_group.trust,
        group_expiration: new_group
            .group_expiration
            .and_then(|i| if i < 1 { None } else { Some(i) }),
    };

    diesel::insert_into(schema::groups::table)
        .values(&group)
        .on_conflict_do_nothing()
        .get_result::<Group>(connection)
        .map_err(Into::into)
        .map(|group| {
            let log_ctx = LogContext::with(group.id, *host_uuid);
            internal::log::db_log(
                connection,
                &log_ctx,
                LogTargetType::Group,
                LogOperationType::Created,
                None,
            );
            group
        })
}

pub fn update_group(
    host_uuid: &Uuid,
    connection: &PgConnection,
    name: String,
    group_update: GroupUpdate,
) -> Result<Group, Error> {
    let log_comment = group_update.log_comment();
    diesel::update(schema::groups::table.filter(schema::groups::name.eq(&name)))
        .set((
            group_update
                .description
                .map(|d| schema::groups::description.eq(d)),
            group_update
                .capabilities
                .map(|c| schema::groups::capabilities.eq(c)),
            group_update.typ.map(|t| schema::groups::typ.eq(t)),
            group_update.trust.map(|t| schema::groups::trust.eq(t)),
            group_update
                .group_expiration
                .map(|e| e.and_then(|i| if i < 1 { None } else { Some(i) }))
                .map(|e| schema::groups::group_expiration.eq(e)),
        ))
        .get_result::<Group>(connection)
        .map_err(Into::into)
        .map(move |group| {
            let log_ctx = LogContext::with(group.id, *host_uuid);
            internal::log::db_log(
                connection,
                &log_ctx,
                LogTargetType::Group,
                LogOperationType::Updated,
                log_comment_body(&log_comment),
            );
            group
        })
}

fn log_delete(
    connection: &PgConnection,
    log_ctx: &LogContext,
    target: LogTargetType,
    body: Option<Value>,
) {
    internal::log::db_log(connection, log_ctx, target, LogOperationType::Deleted, body);
}

pub fn delete_group(host_uuid: &Uuid, connection: &PgConnection, name: &str) -> Result<(), Error> {
    let group = get_group(connection, name)?;
    let log_ctx = LogContext::with(group.id, *host_uuid);
    diesel::delete(schema::invitations::table)
        .filter(schema::invitations::group_id.eq(group.id))
        .execute(connection)
        .optional()
        .map(|_| {
            log_delete(
                connection,
                &log_ctx,
                LogTargetType::Invitation,
                log_comment_body("all outstanding invitations"),
            )
        })?;
    diesel::delete(schema::roles::table)
        .filter(schema::roles::group_id.eq(group.id))
        .execute(connection)
        .map(|_| {
            log_delete(
                connection,
                &log_ctx,
                LogTargetType::Role,
                log_comment_body("all roles"),
            )
        })?;
    diesel::delete(schema::terms::table)
        .filter(schema::terms::group_id.eq(group.id))
        .execute(connection)
        .optional()
        .map(|_| log_delete(connection, &log_ctx, LogTargetType::Terms, None))?;
    diesel::update(schema::groups::table)
        .filter(schema::groups::name.eq(name))
        .set((
            schema::groups::description.eq(""),
            schema::groups::active.eq(false),
        ))
        .execute(connection)
        .map(|_| log_delete(connection, &log_ctx, LogTargetType::Group, None))
        .map_err(Into::into)
}

pub fn groups_for_user(connection: &PgConnection, user_uuid: &Uuid) -> Result<Vec<Group>, Error> {
    schema::memberships::table
        .filter(schema::memberships::user_uuid.eq(user_uuid))
        .select(schema::memberships::group_id)
        .inner_join(schema::groups::table)
        .select(schema::groups::all_columns)
        .get_results::<Group>(connection)
        .map_err(Into::into)
}
