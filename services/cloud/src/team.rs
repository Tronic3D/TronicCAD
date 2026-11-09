//! Teams, members and roles, shared projects, areas and locks, comments,
//! notifications and presence.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use deadpool_postgres::GenericClient;
use serde::Deserialize;
use serde_json::json;
use tronic_project::areas::{changed_areas, Area};
use tronic_storage::Commit;
use uuid::Uuid;

use crate::audit::{self, Entry};
use crate::permissions::{allows, may_assign, Action, TeamRole};
use crate::{authed, blocking, client, normalize_email, token_hash, ApiError, AppState};

const INVITATION_DAYS: i32 = 14;
const LOCK_HOURS: i32 = 4;
const PRESENCE_SECONDS: f64 = 90.0;
const MAX_NAME_CHARS: usize = 100;
const MAX_COMMENT_CHARS: usize = 10_000;

fn role_of_text(text: &str) -> Result<TeamRole, ApiError> {
    TeamRole::parse(text).ok_or_else(|| ApiError::Internal(format!("unknown role {text}")))
}

fn check_name(name: &str) -> Result<String, ApiError> {
    let name = name.trim();
    if name.is_empty()
        || name.chars().count() > MAX_NAME_CHARS
        || name.chars().any(char::is_control)
    {
        return Err(ApiError::BadRequest("invalid_name"));
    }
    Ok(name.to_owned())
}

async fn email_of<C: GenericClient>(db: &C, account: Uuid) -> Result<String, ApiError> {
    Ok(db
        .query_one("SELECT email FROM accounts WHERE id = $1", &[&account])
        .await?
        .get(0))
}

/// The role in a team. None for those outside it.
pub(crate) async fn team_role<C: GenericClient>(
    db: &C,
    team: Uuid,
    account: Uuid,
) -> Result<Option<TeamRole>, ApiError> {
    db.query_opt(
        "SELECT role FROM members WHERE team = $1 AND account = $2",
        &[&team, &account],
    )
    .await?
    .map(|row| role_of_text(row.get(0)))
    .transpose()
}

/// The team of a project and the role there. A project role overrides the team role,
/// but only for members: leaving the team ends all of it.
pub(crate) async fn project_role<C: GenericClient>(
    db: &C,
    repo: Uuid,
    account: Uuid,
) -> Result<Option<(Uuid, Option<TeamRole>)>, ApiError> {
    let Some(row) = db
        .query_opt("SELECT team FROM repos WHERE id = $1", &[&repo])
        .await?
    else {
        return Ok(None);
    };
    let Some(team) = row.get::<_, Option<Uuid>>(0) else {
        return Ok(None);
    };
    let Some(in_team) = team_role(db, team, account).await? else {
        return Ok(Some((team, None)));
    };
    let override_role = db
        .query_opt(
            "SELECT role FROM project_roles WHERE repo = $1 AND account = $2",
            &[&repo, &account],
        )
        .await?
        .map(|row| role_of_text(row.get(0)))
        .transpose()?;
    Ok(Some((team, Some(override_role.unwrap_or(in_team)))))
}

async fn require_team<C: GenericClient>(
    db: &C,
    team: Uuid,
    account: Uuid,
    action: Action,
) -> Result<TeamRole, ApiError> {
    let role = team_role(db, team, account)
        .await?
        .ok_or(ApiError::Forbidden)?;
    if allows(role, action) {
        Ok(role)
    } else {
        Err(ApiError::Forbidden)
    }
}

async fn require_project<C: GenericClient>(
    db: &C,
    repo: Uuid,
    account: Uuid,
    action: Action,
) -> Result<(Uuid, TeamRole), ApiError> {
    let (team, role) = project_role(db, repo, account)
        .await?
        .ok_or(ApiError::NotFound)?;
    let role = role.ok_or(ApiError::Forbidden)?;
    if allows(role, action) {
        Ok((team, role))
    } else {
        Err(ApiError::Forbidden)
    }
}

async fn notify<C: GenericClient>(
    db: &C,
    account: Uuid,
    kind: &str,
    team: Option<Uuid>,
    repo: Option<Uuid>,
    params: serde_json::Value,
) -> Result<(), ApiError> {
    db.execute(
        "INSERT INTO notifications (account, kind, team, repo, params) VALUES ($1, $2, $3, $4, $5)",
        &[&account, &kind, &team, &repo, &params.to_string()],
    )
    .await?;
    Ok(())
}

// ---- Teams ----

#[derive(Deserialize)]
struct NameBody {
    name: String,
}

async fn create_team(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<NameBody>,
) -> Result<Response, ApiError> {
    let account = authed(&state, &headers).await?;
    let name = check_name(&body.name)?;
    let mut db = client(&state).await?;
    let id = Uuid::new_v4();
    let email = email_of(&db, account).await?;
    let tx = db.transaction().await?;
    tx.execute(
        "INSERT INTO teams (id, name) VALUES ($1, $2)",
        &[&id, &name],
    )
    .await?;
    tx.execute(
        "INSERT INTO members (team, account, role) VALUES ($1, $2, 'admin')",
        &[&id, &account],
    )
    .await?;
    audit::record(
        &tx,
        Entry {
            actor: Some(account),
            actor_email: &email,
            team: Some(id),
            action: "team.create",
            detail: &name,
            ..Entry::default()
        },
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(json!({"id": id}))).into_response())
}

async fn list_teams(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let account = authed(&state, &headers).await?;
    let teams: Vec<serde_json::Value> = client(&state)
        .await?
        .query(
            "SELECT t.id, t.name, m.role FROM teams t JOIN members m ON m.team = t.id
             WHERE m.account = $1 ORDER BY t.name",
            &[&account],
        )
        .await?
        .iter()
        .map(|row| {
            json!({"id": row.get::<_, Uuid>(0), "name": row.get::<_, String>(1), "role": row.get::<_, String>(2)})
        })
        .collect();
    Ok(Json(json!({"teams": teams})).into_response())
}

async fn rename_team(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(team): Path<Uuid>,
    Json(body): Json<NameBody>,
) -> Result<StatusCode, ApiError> {
    let account = authed(&state, &headers).await?;
    let name = check_name(&body.name)?;
    let mut db = client(&state).await?;
    require_team(&db, team, account, Action::ManageTeam).await?;
    let email = email_of(&db, account).await?;
    let tx = db.transaction().await?;
    tx.execute("UPDATE teams SET name = $2 WHERE id = $1", &[&team, &name])
        .await?;
    audit::record(
        &tx,
        Entry {
            actor: Some(account),
            actor_email: &email,
            team: Some(team),
            action: "team.rename",
            detail: &name,
            ..Entry::default()
        },
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Ends the team. Its projects stay with those who created them, as private
/// projects, so that nothing is lost by this.
async fn delete_team(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(team): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let account = authed(&state, &headers).await?;
    let mut db = client(&state).await?;
    require_team(&db, team, account, Action::ManageTeam).await?;
    let email = email_of(&db, account).await?;
    let tx = db.transaction().await?;
    audit::record(
        &tx,
        Entry {
            actor: Some(account),
            actor_email: &email,
            team: Some(team),
            action: "team.delete",
            ..Entry::default()
        },
    )
    .await?;
    tx.execute("DELETE FROM teams WHERE id = $1", &[&team])
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- Members ----

async fn list_members(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(team): Path<Uuid>,
) -> Result<Response, ApiError> {
    let account = authed(&state, &headers).await?;
    let db = client(&state).await?;
    require_team(&db, team, account, Action::Read).await?;
    let members: Vec<serde_json::Value> = db
        .query(
            "SELECT a.id, a.email, m.role,
                    EXISTS (SELECT 1 FROM presence p WHERE p.account = a.id
                            AND p.seen > now() - make_interval(secs => $2))
             FROM members m JOIN accounts a ON a.id = m.account
             WHERE m.team = $1 ORDER BY a.email",
            &[&team, &PRESENCE_SECONDS],
        )
        .await?
        .iter()
        .map(|row| {
            json!({
                "id": row.get::<_, Uuid>(0),
                "email": row.get::<_, String>(1),
                "role": row.get::<_, String>(2),
                "online": row.get::<_, bool>(3),
            })
        })
        .collect();
    Ok(Json(json!({"members": members})).into_response())
}

#[derive(Deserialize)]
struct RoleBody {
    role: String,
}

async fn other_admins<C: GenericClient>(db: &C, team: Uuid, except: Uuid) -> Result<i64, ApiError> {
    Ok(db
        .query_one(
            "SELECT count(*) FROM members WHERE team = $1 AND role = 'admin' AND account <> $2",
            &[&team, &except],
        )
        .await?
        .get(0))
}

async fn change_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((team, member)): Path<(Uuid, Uuid)>,
    Json(body): Json<RoleBody>,
) -> Result<StatusCode, ApiError> {
    let account = authed(&state, &headers).await?;
    let new_role = TeamRole::parse(&body.role).ok_or(ApiError::BadRequest("invalid_role"))?;
    let mut db = client(&state).await?;
    let actor = require_team(&db, team, account, Action::ManageMembers).await?;
    let current = team_role(&db, team, member)
        .await?
        .ok_or(ApiError::NotFound)?;
    if !may_assign(actor, current) || !may_assign(actor, new_role) {
        return Err(ApiError::Forbidden);
    }
    let email = email_of(&db, account).await?;
    let target = email_of(&db, member).await?;
    let tx = db.transaction().await?;
    tx.execute("SELECT id FROM teams WHERE id = $1 FOR UPDATE", &[&team])
        .await?;
    if current == TeamRole::Admin
        && new_role != TeamRole::Admin
        && other_admins(&tx, team, member).await? == 0
    {
        return Err(ApiError::Conflict("last_admin"));
    }
    tx.execute(
        "UPDATE members SET role = $3 WHERE team = $1 AND account = $2",
        &[&team, &member, &new_role.name()],
    )
    .await?;
    audit::record(
        &tx,
        Entry {
            actor: Some(account),
            actor_email: &email,
            team: Some(team),
            action: "member.role",
            detail: &format!("{target}: {} -> {}", current.name(), new_role.name()),
            ..Entry::default()
        },
    )
    .await?;
    notify(
        &tx,
        member,
        "role_changed",
        Some(team),
        None,
        json!({"role": new_role.name()}),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((team, member)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    let account = authed(&state, &headers).await?;
    let mut db = client(&state).await?;
    let current = team_role(&db, team, member)
        .await?
        .ok_or(ApiError::NotFound)?;
    // Anyone may leave. Removing others takes the right to manage them.
    if member != account {
        let actor = require_team(&db, team, account, Action::ManageMembers).await?;
        if !may_assign(actor, current) {
            return Err(ApiError::Forbidden);
        }
    }
    let email = email_of(&db, account).await?;
    let target = email_of(&db, member).await?;
    let tx = db.transaction().await?;
    tx.execute("SELECT id FROM teams WHERE id = $1 FOR UPDATE", &[&team])
        .await?;
    if current == TeamRole::Admin && other_admins(&tx, team, member).await? == 0 {
        return Err(ApiError::Conflict("last_admin"));
    }
    tx.execute(
        "DELETE FROM members WHERE team = $1 AND account = $2",
        &[&team, &member],
    )
    .await?;
    tx.execute(
        "DELETE FROM locks WHERE holder = $1 AND repo IN (SELECT id FROM repos WHERE team = $2)",
        &[&member, &team],
    )
    .await?;
    audit::record(
        &tx,
        Entry {
            actor: Some(account),
            actor_email: &email,
            team: Some(team),
            action: "member.remove",
            detail: &target,
            ..Entry::default()
        },
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- Invitations ----

#[derive(Deserialize)]
struct InviteBody {
    email: String,
    role: String,
}

/// Creates an invitation. There is no mail service, so the code is handed back to be
/// passed on by hand. It is only valid for this address and stored only as a hash.
async fn invite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(team): Path<Uuid>,
    Json(body): Json<InviteBody>,
) -> Result<Response, ApiError> {
    let account = authed(&state, &headers).await?;
    let email = normalize_email(&body.email)?;
    let role = TeamRole::parse(&body.role).ok_or(ApiError::BadRequest("invalid_role"))?;
    let mut db = client(&state).await?;
    let actor = require_team(&db, team, account, Action::ManageMembers).await?;
    if !may_assign(actor, role) {
        return Err(ApiError::Forbidden);
    }
    let random: [u8; 24] = rand::random();
    let code: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    let by = email_of(&db, account).await?;
    let tx = db.transaction().await?;
    tx.execute(
        "INSERT INTO invitations (code_hash, team, email, role, expires, invited_by)
         VALUES ($1, $2, $3, $4, now() + make_interval(days => $5), $6)",
        &[
            &token_hash(&code),
            &team,
            &email,
            &role.name(),
            &INVITATION_DAYS,
            &account,
        ],
    )
    .await?;
    audit::record(
        &tx,
        Entry {
            actor: Some(account),
            actor_email: &by,
            team: Some(team),
            action: "invitation.create",
            detail: &format!("{email} as {}", role.name()),
            ..Entry::default()
        },
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(json!({"code": code}))).into_response())
}

async fn list_invitations(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(team): Path<Uuid>,
) -> Result<Response, ApiError> {
    let account = authed(&state, &headers).await?;
    let db = client(&state).await?;
    require_team(&db, team, account, Action::ManageMembers).await?;
    let list: Vec<serde_json::Value> = db
        .query(
            "SELECT email, role FROM invitations
             WHERE team = $1 AND used_at IS NULL AND expires > now() ORDER BY email",
            &[&team],
        )
        .await?
        .iter()
        .map(|row| json!({"email": row.get::<_, String>(0), "role": row.get::<_, String>(1)}))
        .collect();
    Ok(Json(json!({"invitations": list})).into_response())
}

#[derive(Deserialize)]
struct EmailBody {
    email: String,
}

async fn revoke_invitation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(team): Path<Uuid>,
    Json(body): Json<EmailBody>,
) -> Result<StatusCode, ApiError> {
    let account = authed(&state, &headers).await?;
    let email = normalize_email(&body.email)?;
    let mut db = client(&state).await?;
    require_team(&db, team, account, Action::ManageMembers).await?;
    let by = email_of(&db, account).await?;
    let tx = db.transaction().await?;
    tx.execute(
        "DELETE FROM invitations WHERE team = $1 AND email = $2 AND used_at IS NULL",
        &[&team, &email],
    )
    .await?;
    audit::record(
        &tx,
        Entry {
            actor: Some(account),
            actor_email: &by,
            team: Some(team),
            action: "invitation.revoke",
            detail: &email,
            ..Entry::default()
        },
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct CodeBody {
    code: String,
}

async fn accept_invitation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CodeBody>,
) -> Result<Response, ApiError> {
    let account = authed(&state, &headers).await?;
    let mut db = client(&state).await?;
    let email = email_of(&db, account).await?;
    let tx = db.transaction().await?;
    let found = tx
        .query_opt(
            "SELECT team, role, invited_by FROM invitations
             WHERE code_hash = $1 AND email = $2 AND used_at IS NULL AND expires > now() FOR UPDATE",
            &[&token_hash(&body.code), &email],
        )
        .await?
        .ok_or(ApiError::BadRequest("invalid_invitation"))?;
    let (team, role, by): (Uuid, String, Uuid) = (found.get(0), found.get(1), found.get(2));
    tx.execute(
        "UPDATE invitations SET used_at = now() WHERE code_hash = $1",
        &[&token_hash(&body.code)],
    )
    .await?;
    tx.execute(
        "INSERT INTO members (team, account, role) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        &[&team, &account, &role],
    )
    .await?;
    audit::record(
        &tx,
        Entry {
            actor: Some(account),
            actor_email: &email,
            team: Some(team),
            action: "invitation.accept",
            detail: &role,
            ..Entry::default()
        },
    )
    .await?;
    notify(
        &tx,
        by,
        "invitation_accepted",
        Some(team),
        None,
        json!({"email": email}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"team": team, "role": role})).into_response())
}

// ---- Projects ----

#[derive(Deserialize)]
struct ProjectBody {
    repo: Uuid,
    name: String,
}

/// Adds a project to the team: a new one, or one this account owns already.
async fn add_project(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(team): Path<Uuid>,
    Json(body): Json<ProjectBody>,
) -> Result<StatusCode, ApiError> {
    let account = authed(&state, &headers).await?;
    let name = check_name(&body.name)?;
    let mut db = client(&state).await?;
    require_team(&db, team, account, Action::CreateProject).await?;
    let email = email_of(&db, account).await?;
    let tx = db.transaction().await?;
    tx.execute(
        "INSERT INTO repos (id, owner) VALUES ($1, $2) ON CONFLICT (id) DO NOTHING",
        &[&body.repo, &account],
    )
    .await?;
    let row = tx
        .query_one(
            "SELECT owner, team FROM repos WHERE id = $1 FOR UPDATE",
            &[&body.repo],
        )
        .await?;
    let (owner, existing): (Uuid, Option<Uuid>) = (row.get(0), row.get(1));
    if owner != account || existing.is_some_and(|other| other != team) {
        return Err(ApiError::Forbidden);
    }
    tx.execute(
        "UPDATE repos SET team = $2, name = $3 WHERE id = $1",
        &[&body.repo, &team, &name],
    )
    .await?;
    tx.execute("DELETE FROM shares WHERE repo = $1", &[&body.repo])
        .await?;
    audit::record(
        &tx,
        Entry {
            actor: Some(account),
            actor_email: &email,
            team: Some(team),
            repo: Some(body.repo),
            action: "project.add",
            detail: &name,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::CREATED)
}

async fn list_projects(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(team): Path<Uuid>,
) -> Result<Response, ApiError> {
    let account = authed(&state, &headers).await?;
    let db = client(&state).await?;
    require_team(&db, team, account, Action::Read).await?;
    let list: Vec<serde_json::Value> = db
        .query(
            "SELECT id, coalesce(name, '') FROM repos WHERE team = $1 ORDER BY name",
            &[&team],
        )
        .await?
        .iter()
        .map(|row| json!({"id": row.get::<_, Uuid>(0), "name": row.get::<_, String>(1)}))
        .collect();
    Ok(Json(json!({"projects": list})).into_response())
}

async fn granted_areas<C: GenericClient>(
    db: &C,
    repo: Uuid,
    account: Uuid,
) -> Result<Vec<String>, ApiError> {
    Ok(db
        .query(
            "SELECT area FROM area_grants WHERE repo = $1 AND account = $2 ORDER BY area",
            &[&repo, &account],
        )
        .await?
        .iter()
        .map(|row| row.get(0))
        .collect())
}

/// What this account may do in this project, for the interface to show. The server
/// decides again on every request.
async fn access(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(repo): Path<Uuid>,
) -> Result<Response, ApiError> {
    let account = authed(&state, &headers).await?;
    let db = client(&state).await?;
    let (team, role) = require_project(&db, repo, account, Action::Read).await?;
    let can: Vec<Action> = Action::ALL
        .into_iter()
        .filter(|action| allows(role, *action))
        .collect();
    Ok(Json(json!({
        "team": team,
        "role": role,
        "can": can,
        "areas": granted_areas(&db, repo, account).await?,
    }))
    .into_response())
}

#[derive(Deserialize)]
struct ProjectRoleBody {
    account: Uuid,
    role: Option<String>,
}

async fn set_project_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(repo): Path<Uuid>,
    Json(body): Json<ProjectRoleBody>,
) -> Result<StatusCode, ApiError> {
    let account = authed(&state, &headers).await?;
    let mut db = client(&state).await?;
    let (team, actor) = require_project(&db, repo, account, Action::ManageProject).await?;
    let in_team = team_role(&db, team, body.account)
        .await?
        .ok_or(ApiError::NotFound)?;
    if !may_assign(actor, in_team) {
        return Err(ApiError::Forbidden);
    }
    let role = body
        .role
        .as_deref()
        .map(|text| TeamRole::parse(text).ok_or(ApiError::BadRequest("invalid_role")))
        .transpose()?;
    if let Some(role) = role {
        if !may_assign(actor, role) {
            return Err(ApiError::Forbidden);
        }
    }
    let email = email_of(&db, account).await?;
    let target = email_of(&db, body.account).await?;
    let tx = db.transaction().await?;
    match role {
        Some(role) => {
            tx.execute(
                "INSERT INTO project_roles (repo, account, role) VALUES ($1, $2, $3)
                 ON CONFLICT (repo, account) DO UPDATE SET role = $3",
                &[&repo, &body.account, &role.name()],
            )
            .await?;
        }
        None => {
            tx.execute(
                "DELETE FROM project_roles WHERE repo = $1 AND account = $2",
                &[&repo, &body.account],
            )
            .await?;
        }
    }
    audit::record(
        &tx,
        Entry {
            actor: Some(account),
            actor_email: &email,
            team: Some(team),
            repo: Some(repo),
            action: "project.role",
            detail: &format!("{target}: {}", role.map_or("team role", TeamRole::name)),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct GrantBody {
    account: Uuid,
    area: String,
}

fn parse_area(text: &str) -> Result<Area, ApiError> {
    Area::parse(text).ok_or(ApiError::BadRequest("invalid_area"))
}

async fn set_grant(
    state: AppState,
    headers: HeaderMap,
    repo: Uuid,
    body: GrantBody,
    add: bool,
) -> Result<StatusCode, ApiError> {
    let account = authed(&state, &headers).await?;
    let area = parse_area(&body.area)?;
    let mut db = client(&state).await?;
    let (team, _) = require_project(&db, repo, account, Action::ManageProject).await?;
    team_role(&db, team, body.account)
        .await?
        .ok_or(ApiError::NotFound)?;
    let email = email_of(&db, account).await?;
    let target = email_of(&db, body.account).await?;
    let tx = db.transaction().await?;
    if add {
        tx.execute(
            "INSERT INTO area_grants (repo, account, area) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
            &[&repo, &body.account, &area.name()],
        )
        .await?;
    } else {
        tx.execute(
            "DELETE FROM area_grants WHERE repo = $1 AND account = $2 AND area = $3",
            &[&repo, &body.account, &area.name()],
        )
        .await?;
    }
    audit::record(
        &tx,
        Entry {
            actor: Some(account),
            actor_email: &email,
            team: Some(team),
            repo: Some(repo),
            action: if add { "area.grant" } else { "area.revoke" },
            detail: &format!("{target}: {}", area.name()),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_grants(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(repo): Path<Uuid>,
) -> Result<Response, ApiError> {
    let account = authed(&state, &headers).await?;
    let db = client(&state).await?;
    require_project(&db, repo, account, Action::ManageProject).await?;
    let list: Vec<serde_json::Value> = db
        .query(
            "SELECT account, area FROM area_grants WHERE repo = $1 ORDER BY account, area",
            &[&repo],
        )
        .await?
        .iter()
        .map(|row| json!({"account": row.get::<_, Uuid>(0), "area": row.get::<_, String>(1)}))
        .collect();
    Ok(Json(json!({"grants": list})).into_response())
}

async fn add_grant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(repo): Path<Uuid>,
    Json(body): Json<GrantBody>,
) -> Result<StatusCode, ApiError> {
    set_grant(state, headers, repo, body, true).await
}

async fn remove_grant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(repo): Path<Uuid>,
    Json(body): Json<GrantBody>,
) -> Result<StatusCode, ApiError> {
    set_grant(state, headers, repo, body, false).await
}

// ---- Locks ----

/// Whether this person may change this area at all, leaving locks aside.
async fn may_write_area<C: GenericClient>(
    db: &C,
    repo: Uuid,
    account: Uuid,
    role: TeamRole,
    area: Area,
) -> Result<bool, ApiError> {
    if allows(role, Action::WriteAnyArea) {
        return Ok(true);
    }
    if !allows(role, Action::WriteAssignedArea) {
        return Ok(false);
    }
    Ok(granted_areas(db, repo, account)
        .await?
        .iter()
        .any(|name| name == area.name()))
}

async fn list_locks(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(repo): Path<Uuid>,
) -> Result<Response, ApiError> {
    let account = authed(&state, &headers).await?;
    let db = client(&state).await?;
    require_project(&db, repo, account, Action::Read).await?;
    let list: Vec<serde_json::Value> = db
        .query(
            "SELECT l.area, l.holder, a.email, to_char(l.expires AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')
             FROM locks l JOIN accounts a ON a.id = l.holder
             WHERE l.repo = $1 AND l.expires > now() ORDER BY l.area",
            &[&repo],
        )
        .await?
        .iter()
        .map(|row| {
            json!({
                "area": row.get::<_, String>(0),
                "holder": row.get::<_, Uuid>(1),
                "email": row.get::<_, String>(2),
                "expires": row.get::<_, String>(3),
                "mine": row.get::<_, Uuid>(1) == account,
            })
        })
        .collect();
    Ok(Json(json!({"locks": list})).into_response())
}

#[derive(Deserialize)]
struct AreaBody {
    area: String,
}

async fn take_lock(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(repo): Path<Uuid>,
    Json(body): Json<AreaBody>,
) -> Result<StatusCode, ApiError> {
    let account = authed(&state, &headers).await?;
    let area = parse_area(&body.area)?;
    let mut db = client(&state).await?;
    let (team, role) = require_project(&db, repo, account, Action::Read).await?;
    if !may_write_area(&db, repo, account, role, area).await? {
        return Err(ApiError::AreaDenied(area.name()));
    }
    let email = email_of(&db, account).await?;
    let tx = db.transaction().await?;
    // Taking a free or expired lock, or renewing one's own. All in one statement, so
    // that two people asking at the same moment cannot both get it.
    let taken = tx
        .execute(
            "INSERT INTO locks (repo, area, holder, expires)
             VALUES ($1, $2, $3, now() + make_interval(hours => $4))
             ON CONFLICT (repo, area) DO UPDATE
             SET holder = excluded.holder, expires = excluded.expires
             WHERE locks.expires <= now() OR locks.holder = excluded.holder",
            &[&repo, &area.name(), &account, &LOCK_HOURS],
        )
        .await?;
    if taken == 0 {
        let holder: String = tx
            .query_one(
                "SELECT a.email FROM locks l JOIN accounts a ON a.id = l.holder
                 WHERE l.repo = $1 AND l.area = $2",
                &[&repo, &area.name()],
            )
            .await?
            .get(0);
        return Err(ApiError::Locked {
            area: area.name(),
            holder,
        });
    }
    audit::record(
        &tx,
        Entry {
            actor: Some(account),
            actor_email: &email,
            team: Some(team),
            repo: Some(repo),
            action: "lock.take",
            detail: area.name(),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn release_lock(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((repo, area)): Path<(Uuid, String)>,
) -> Result<StatusCode, ApiError> {
    let account = authed(&state, &headers).await?;
    let area = parse_area(&area)?;
    let mut db = client(&state).await?;
    let (team, role) = require_project(&db, repo, account, Action::Read).await?;
    let email = email_of(&db, account).await?;
    let tx = db.transaction().await?;
    let holder: Option<Uuid> = tx
        .query_opt(
            "SELECT holder FROM locks WHERE repo = $1 AND area = $2 FOR UPDATE",
            &[&repo, &area.name()],
        )
        .await?
        .map(|row| row.get(0));
    let Some(holder) = holder else {
        return Ok(StatusCode::NO_CONTENT);
    };
    let overriding = holder != account;
    if overriding && !allows(role, Action::ManageProject) {
        return Err(ApiError::Forbidden);
    }
    tx.execute(
        "DELETE FROM locks WHERE repo = $1 AND area = $2",
        &[&repo, &area.name()],
    )
    .await?;
    audit::record(
        &tx,
        Entry {
            actor: Some(account),
            actor_email: &email,
            team: Some(team),
            repo: Some(repo),
            action: if overriding {
                "lock.override"
            } else {
                "lock.release"
            },
            detail: area.name(),
        },
    )
    .await?;
    if overriding {
        notify(
            &tx,
            holder,
            "lock_overridden",
            Some(team),
            Some(repo),
            json!({"area": area.name(), "by": email}),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Decides whether a new version may enter a team project, from what it really
/// changes. The client's word does not count: the stored files are compared, so
/// neither a modified client nor a mistake can get past a lock or a role.
pub(crate) async fn check_commit(
    state: &AppState,
    repo: Uuid,
    account: Uuid,
    role: TeamRole,
    body: &[u8],
) -> Result<Vec<&'static str>, ApiError> {
    if !allows(role, Action::WriteAssignedArea) {
        return Err(ApiError::Forbidden);
    }
    let commit: Commit =
        serde_json::from_slice(body).map_err(|_| ApiError::BadRequest("invalid_commit"))?;
    let worker = state.clone();
    let changed = blocking(move || -> Result<Vec<Area>, ApiError> {
        let provider = worker.0.store.provider();
        let read_blob = |hash: &str| {
            provider
                .get(&format!("{repo}/blobs/{hash}"))
                .map_err(ApiError::from)?
                .ok_or(ApiError::BadRequest("blob_missing"))
        };
        let new = read_blob(&commit.blob)?;
        if commit.parents.is_empty() {
            return Ok(Area::ALL.to_vec());
        }
        // A change is what differs from every parent: a merge that only takes over
        // a parent's state changes nothing new.
        let mut remaining: Vec<Area> = Area::ALL.to_vec();
        for parent in &commit.parents {
            let stored = provider
                .get(&format!("{repo}/commits/{parent}.json"))
                .map_err(ApiError::from)?
                .ok_or(ApiError::BadRequest("parent_missing"))?;
            let parent_commit: Commit = serde_json::from_slice(&stored)
                .map_err(|_| ApiError::BadRequest("invalid_commit"))?;
            let old = read_blob(&parent_commit.blob)?;
            let changed =
                changed_areas(&old, &new).map_err(|_| ApiError::BadRequest("invalid_project"))?;
            remaining.retain(|area| changed.contains(area));
        }
        Ok(remaining)
    })
    .await??;

    let db = client(state).await?;
    for area in &changed {
        if !may_write_area(&db, repo, account, role, *area).await? {
            return Err(ApiError::AreaDenied(area.name()));
        }
        let holder = db
            .query_opt(
                "SELECT l.holder, a.email FROM locks l JOIN accounts a ON a.id = l.holder
                 WHERE l.repo = $1 AND l.area = $2 AND l.expires > now()",
                &[&repo, &area.name()],
            )
            .await?;
        if let Some(row) = holder {
            if row.get::<_, Uuid>(0) != account {
                return Err(ApiError::Locked {
                    area: area.name(),
                    holder: row.get(1),
                });
            }
        }
    }
    Ok(changed.iter().map(|area| area.name()).collect())
}

// ---- Comments ----

#[derive(Deserialize)]
struct CommentBody {
    area: String,
    anchor: String,
    body: String,
}

async fn add_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(repo): Path<Uuid>,
    Json(body): Json<CommentBody>,
) -> Result<Response, ApiError> {
    let account = authed(&state, &headers).await?;
    let area = parse_area(&body.area)?;
    let text = body.body.trim();
    if text.is_empty()
        || text.chars().count() > MAX_COMMENT_CHARS
        || body.anchor.chars().count() > MAX_NAME_CHARS
    {
        return Err(ApiError::BadRequest("invalid_comment"));
    }
    let mut db = client(&state).await?;
    let (team, _) = require_project(&db, repo, account, Action::Comment).await?;
    let email = email_of(&db, account).await?;
    let id = Uuid::new_v4();
    let others: Vec<Uuid> = db
        .query(
            "SELECT account FROM members WHERE team = $1 AND account <> $2",
            &[&team, &account],
        )
        .await?
        .iter()
        .map(|row| row.get(0))
        .collect();
    let tx = db.transaction().await?;
    tx.execute(
        "INSERT INTO comments (id, repo, author, author_email, area, anchor, body)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
        &[
            &id,
            &repo,
            &account,
            &email,
            &area.name(),
            &body.anchor,
            &text,
        ],
    )
    .await?;
    audit::record(
        &tx,
        Entry {
            actor: Some(account),
            actor_email: &email,
            team: Some(team),
            repo: Some(repo),
            action: "comment.add",
            detail: &id.to_string(),
        },
    )
    .await?;
    for other in others {
        notify(
            &tx,
            other,
            "comment",
            Some(team),
            Some(repo),
            json!({"by": email, "area": area.name()}),
        )
        .await?;
    }
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(json!({"id": id}))).into_response())
}

async fn list_comments(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(repo): Path<Uuid>,
) -> Result<Response, ApiError> {
    let account = authed(&state, &headers).await?;
    let db = client(&state).await?;
    require_project(&db, repo, account, Action::Read).await?;
    let list: Vec<serde_json::Value> = db
        .query(
            "SELECT id, author_email, area, anchor, body,
                    to_char(created AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"'), resolved
             FROM comments WHERE repo = $1 ORDER BY created DESC LIMIT 500",
            &[&repo],
        )
        .await?
        .iter()
        .map(|row| {
            json!({
                "id": row.get::<_, Uuid>(0),
                "author": row.get::<_, String>(1),
                "area": row.get::<_, String>(2),
                "anchor": row.get::<_, String>(3),
                "body": row.get::<_, String>(4),
                "created": row.get::<_, String>(5),
                "resolved": row.get::<_, bool>(6),
            })
        })
        .collect();
    Ok(Json(json!({"comments": list})).into_response())
}

async fn resolve_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((repo, comment)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    let account = authed(&state, &headers).await?;
    let mut db = client(&state).await?;
    let (team, role) = require_project(&db, repo, account, Action::Comment).await?;
    let author: Uuid = db
        .query_opt(
            "SELECT author FROM comments WHERE id = $1 AND repo = $2",
            &[&comment, &repo],
        )
        .await?
        .ok_or(ApiError::NotFound)?
        .get(0);
    if author != account && !allows(role, Action::ManageProject) {
        return Err(ApiError::Forbidden);
    }
    let email = email_of(&db, account).await?;
    let tx = db.transaction().await?;
    tx.execute(
        "UPDATE comments SET resolved = true WHERE id = $1",
        &[&comment],
    )
    .await?;
    audit::record(
        &tx,
        Entry {
            actor: Some(account),
            actor_email: &email,
            team: Some(team),
            repo: Some(repo),
            action: "comment.resolve",
            detail: &comment.to_string(),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- Notifications and presence ----

async fn list_notifications(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let account = authed(&state, &headers).await?;
    let list: Vec<serde_json::Value> = client(&state)
        .await?
        .query(
            "SELECT id, kind, params, to_char(created AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"'), read
             FROM notifications WHERE account = $1 ORDER BY id DESC LIMIT 200",
            &[&account],
        )
        .await?
        .iter()
        .map(|row| {
            let params: serde_json::Value =
                serde_json::from_str(row.get::<_, &str>(2)).unwrap_or(serde_json::Value::Null);
            json!({
                "id": row.get::<_, i64>(0),
                "kind": row.get::<_, String>(1),
                "params": params,
                "created": row.get::<_, String>(3),
                "read": row.get::<_, bool>(4),
            })
        })
        .collect();
    Ok(Json(json!({"notifications": list})).into_response())
}

async fn read_notifications(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let account = authed(&state, &headers).await?;
    client(&state)
        .await?
        .execute(
            "UPDATE notifications SET read = true WHERE account = $1",
            &[&account],
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_presence(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(repo): Path<Uuid>,
    Json(body): Json<AreaBody>,
) -> Result<StatusCode, ApiError> {
    let account = authed(&state, &headers).await?;
    let area = parse_area(&body.area)?;
    let db = client(&state).await?;
    require_project(&db, repo, account, Action::Read).await?;
    db.execute(
        "INSERT INTO presence (repo, account, area, seen) VALUES ($1, $2, $3, now())
         ON CONFLICT (repo, account) DO UPDATE SET area = $3, seen = now()",
        &[&repo, &account, &area.name()],
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_presence(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(repo): Path<Uuid>,
) -> Result<Response, ApiError> {
    let account = authed(&state, &headers).await?;
    let db = client(&state).await?;
    require_project(&db, repo, account, Action::Read).await?;
    let list: Vec<serde_json::Value> = db
        .query(
            "SELECT a.email, p.area FROM presence p JOIN accounts a ON a.id = p.account
             WHERE p.repo = $1 AND p.seen > now() - make_interval(secs => $2) ORDER BY a.email",
            &[&repo, &PRESENCE_SECONDS],
        )
        .await?
        .iter()
        .map(|row| json!({"email": row.get::<_, String>(0), "area": row.get::<_, String>(1)}))
        .collect();
    Ok(Json(json!({"presence": list})).into_response())
}

// ---- Audit ----

async fn team_audit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(team): Path<Uuid>,
) -> Result<Response, ApiError> {
    let account = authed(&state, &headers).await?;
    let db = client(&state).await?;
    require_team(&db, team, account, Action::ViewAudit).await?;
    Ok(Json(json!({
        "entries": audit::list_team(&db, team).await?,
        "first_damaged": audit::verify(&db).await?,
    }))
    .into_response())
}

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/teams", post(create_team).get(list_teams))
        .route("/v1/teams/{team}", patch(rename_team).delete(delete_team))
        .route("/v1/teams/{team}/members", get(list_members))
        .route(
            "/v1/teams/{team}/members/{member}",
            patch(change_member).delete(remove_member),
        )
        .route(
            "/v1/teams/{team}/invitations",
            post(invite).get(list_invitations).delete(revoke_invitation),
        )
        .route("/v1/invitations/accept", post(accept_invitation))
        .route(
            "/v1/teams/{team}/projects",
            post(add_project).get(list_projects),
        )
        .route("/v1/teams/{team}/audit", get(team_audit))
        .route("/v1/projects/{repo}/access", get(access))
        .route("/v1/projects/{repo}/roles", post(set_project_role))
        .route(
            "/v1/projects/{repo}/grants",
            get(list_grants).post(add_grant).delete(remove_grant),
        )
        .route("/v1/projects/{repo}/locks", get(list_locks).post(take_lock))
        .route("/v1/projects/{repo}/locks/{area}", delete(release_lock))
        .route(
            "/v1/projects/{repo}/presence",
            get(get_presence).post(set_presence),
        )
        .route(
            "/v1/projects/{repo}/comments",
            get(list_comments).post(add_comment),
        )
        .route(
            "/v1/projects/{repo}/comments/{comment}/resolve",
            post(resolve_comment),
        )
        .route("/v1/notifications", get(list_notifications))
        .route("/v1/notifications/read", post(read_notifications))
}
