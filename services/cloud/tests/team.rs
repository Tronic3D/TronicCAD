//! Teams against a real PostgreSQL: the five roles, locks that hold on real project
//! files, and the audit log. Skipped without `TRONICCAD_TEST_DATABASE_URL`.
#![allow(clippy::unwrap_used, clippy::panic)]

use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use tronic_cloud::permissions::{allows, Action, TeamRole};
use tronic_cloud::{connect, router, Config, Store};
use tronic_core::Project;
use tronic_pcb::{Board, MM};
use tronic_project::areas::{merge, Merge};
use tronic_project::format::{decode, encode, Document};
use tronic_schematic::{EditCommand, Editor, Point, Schematic};
use tronic_storage::*;

fn database() -> Option<String> {
    std::env::var("TRONICCAD_TEST_DATABASE_URL").ok()
}

macro_rules! needs_database {
    () => {
        if database().is_none() {
            eprintln!("skipped: TRONICCAD_TEST_DATABASE_URL is not set");
            return;
        }
    };
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique() -> String {
    format!(
        "{}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

fn new_id() -> String {
    let random: [u8; 16] = rand::random();
    let hex: String = random.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-4{}-8{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[13..16],
        &hex[17..20],
        &hex[20..32]
    )
}

const PASSWORD: &str = "a good long test password";

async fn instance_on(url: &str, quota: i64, registration: bool) -> String {
    let store = std::env::temp_dir().join(format!("troniccad-team-{}", unique()));
    std::fs::create_dir_all(&store).unwrap();
    let state = connect(
        url,
        Store::folder(store),
        Config {
            default_quota: quota,
            allow_registration: registration,
        },
    )
    .await
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });
    address
}

async fn instance() -> String {
    instance_on(&database().unwrap(), 1 << 32, true).await
}

async fn off<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(work).await.unwrap()
}

/// A raw request. Returns the status and the answer.
fn call(method: &str, url: &str, token: Option<&str>, body: Option<Value>) -> (u16, Value) {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .allow_non_standard_methods(true)
        .build()
        .into();
    let mut builder = ureq::http::Request::builder()
        .method(method)
        .uri(url)
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    let text = body.map(|body| body.to_string()).unwrap_or_default();
    let mut response = agent.run(builder.body(text).unwrap()).unwrap();
    let status = response.status().as_u16();
    let answer = response.body_mut().read_to_string().unwrap_or_default();
    (status, serde_json::from_str(&answer).unwrap_or(Value::Null))
}

#[derive(Clone)]
struct User {
    id: String,
    email: String,
    token: String,
}

#[derive(Clone)]
struct Bench {
    url: String,
    team: String,
    admin: User,
}

impl Bench {
    fn get(&self, path: &str, user: &User) -> (u16, Value) {
        call(
            "GET",
            &format!("{}{path}", self.url),
            Some(&user.token),
            None,
        )
    }
    fn post(&self, path: &str, user: &User, body: Value) -> (u16, Value) {
        call(
            "POST",
            &format!("{}{path}", self.url),
            Some(&user.token),
            Some(body),
        )
    }
    fn patch(&self, path: &str, user: &User, body: Value) -> (u16, Value) {
        call(
            "PATCH",
            &format!("{}{path}", self.url),
            Some(&user.token),
            Some(body),
        )
    }
    fn delete(&self, path: &str, user: &User) -> (u16, Value) {
        call(
            "DELETE",
            &format!("{}{path}", self.url),
            Some(&user.token),
            None,
        )
    }

    fn new_account(&self) -> User {
        let email = format!("user-{}@example.test", unique());
        let token = CloudProvider::register(&self.url, &email, PASSWORD).unwrap();
        let (_, teams) = call(
            "GET",
            &format!("{}/v1/account", self.url),
            Some(&token),
            None,
        );
        assert_eq!(teams["email"], json!(email));
        User {
            id: String::new(),
            email,
            token,
        }
    }

    /// An account that became a member the regular way: invited, then accepted.
    fn member(&self, role: &str) -> User {
        let mut user = self.new_account();
        let (status, invitation) = self.post(
            &format!("/v1/teams/{}/invitations", self.team),
            &self.admin,
            json!({"email": user.email, "role": role}),
        );
        assert_eq!(status, 201, "{invitation}");
        let (status, _) = self.post(
            "/v1/invitations/accept",
            &user,
            json!({"code": invitation["code"]}),
        );
        assert_eq!(status, 200);
        let (_, members) = self.get(&format!("/v1/teams/{}/members", self.team), &self.admin);
        user.id = members["members"]
            .as_array()
            .unwrap()
            .iter()
            .find(|member| member["email"] == json!(user.email))
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        user
    }

    fn project(&self) -> String {
        let id = new_id();
        let (status, answer) = self.post(
            &format!("/v1/teams/{}/projects", self.team),
            &self.admin,
            json!({"repo": id, "name": "Project"}),
        );
        assert_eq!(status, 201, "{answer}");
        id
    }

    fn provider(&self, user: &User) -> CloudProvider {
        CloudProvider::new(&self.url, &user.token).unwrap()
    }

    /// Stores a state on top of whatever is newest, as a client does.
    fn push(&self, user: &User, project: &str, bytes: &[u8]) -> Result<String, StorageError> {
        let provider = self.provider(user);
        let repo = Repo::new(&provider, project);
        let mut state = LinkState::default();
        repo.resolve_keep_local(bytes, "device", &mut state)
    }

    fn newest(&self, project: &str) -> Vec<u8> {
        let provider = self.provider(&self.admin);
        let repo = Repo::new(&provider, project);
        let mut state = LinkState::default();
        repo.refresh(&mut state).unwrap();
        let heads = Repo::heads(&state);
        repo.read(&state, &heads[0]).unwrap()
    }
}

async fn new_bench() -> Bench {
    let url = instance().await;
    off(move || {
        let email = format!("admin-{}@example.test", unique());
        let token = CloudProvider::register(&url, &email, PASSWORD).unwrap();
        let admin = User {
            id: String::new(),
            email,
            token,
        };
        let (status, team) = call(
            "POST",
            &format!("{url}/v1/teams"),
            Some(&admin.token),
            Some(json!({"name": "Team"})),
        );
        assert_eq!(status, 201);
        Bench {
            url,
            team: team["id"].as_str().unwrap().to_owned(),
            admin,
        }
    })
    .await
}

// ---- Project files ----

fn document() -> Document {
    Document {
        project: Project::new("Name", "", time::OffsetDateTime::now_utc()).unwrap(),
        schematic: Schematic::default(),
        board: Board::default(),
    }
}

fn file(document: &Document) -> Vec<u8> {
    encode(
        &document.project,
        &document.schematic,
        &document.board,
        Path::new("x"),
    )
    .unwrap()
}

fn with_schematic(document: &Document, name: &str) -> Document {
    let mut changed = document.clone();
    let mut editor = Editor::new(changed.schematic.clone());
    let sheet = editor.doc().sheets[0].id;
    editor
        .apply(EditCommand::RenameSheet {
            sheet,
            name: name.to_owned(),
        })
        .unwrap();
    changed.schematic = editor.doc().clone();
    changed
}

fn with_board(document: &Document, size: i64) -> Document {
    let mut changed = document.clone();
    changed.board.outline = vec![
        Point::new(0, 0),
        Point::new(size * MM, 0),
        Point::new(size * MM, size * MM),
    ];
    changed
}

fn with_mechanics(document: &Document, nm: i64) -> Document {
    let mut changed = document.clone();
    changed.board.mechanics.thickness = nm;
    changed
}

// ---- The five roles ----

/// Every role against every action, through the real service, compared with the table.
#[tokio::test(flavor = "multi_thread")]
async fn all_five_roles_can_do_exactly_what_the_table_says() {
    needs_database!();
    let bench = new_bench().await;
    off(move || {
        for role in TeamRole::ALL {
            let user = bench.member(role.name());
            let may = |action| allows(role, action);
            let expect = |action: Action, what: &str, status: u16, ok: u16| {
                let wanted = if may(action) { ok } else { 403 };
                assert_eq!(status, wanted, "{role:?} {what}");
            };
            let team = format!("/v1/teams/{}", bench.team);

            // Everyone in the team reads.
            assert_eq!(
                bench.get(&format!("{team}/members"), &user).0,
                200,
                "{role:?}"
            );
            assert_eq!(
                bench.get(&format!("{team}/projects"), &user).0,
                200,
                "{role:?}"
            );

            expect(
                Action::ManageTeam,
                "rename",
                bench.patch(&team, &user, json!({"name": "Team"})).0,
                204,
            );
            let (status, _) = bench.post(
                &format!("{team}/invitations"),
                &user,
                json!({"email": format!("guest-{}@example.test", unique()), "role": "viewer"}),
            );
            expect(Action::ManageMembers, "invite", status, 201);
            let (status, _) = bench.post(
                &format!("{team}/projects"),
                &user,
                json!({"repo": new_id(), "name": "Theirs"}),
            );
            expect(Action::CreateProject, "add project", status, 201);
            expect(
                Action::ViewAudit,
                "audit",
                bench.get(&format!("{team}/audit"), &user).0,
                200,
            );

            let project = bench.project();
            let base = document();
            bench.push(&bench.admin, &project, &file(&base)).unwrap();
            let path = format!("/v1/projects/{project}");
            assert_eq!(
                bench.get(&format!("{path}/access"), &user).0,
                200,
                "{role:?}"
            );
            assert_eq!(
                bench.get(&format!("{path}/locks"), &user).0,
                200,
                "{role:?}"
            );
            assert_eq!(
                bench.get(&format!("{path}/comments"), &user).0,
                200,
                "{role:?}"
            );
            assert_eq!(
                bench
                    .provider(&user)
                    .list(&format!("projects/{project}/commits"))
                    .unwrap()
                    .len(),
                1,
                "{role:?} reads versions"
            );

            let grant = json!({"account": user.id, "area": "board"});
            let (status, _) = bench.post(&format!("{path}/grants"), &user, grant);
            expect(Action::ManageProject, "grant", status, 204);
            let (status, _) = bench.post(
                &format!("{path}/comments"),
                &user,
                json!({"area": "schematic", "anchor": "R1", "body": "text"}),
            );
            expect(Action::Comment, "comment", status, 201);

            // Writing without any assignment.
            let result = bench.push(&user, &project, &file(&with_schematic(&base, "S")));
            let (status, answer) = bench.post(
                &format!("{path}/locks"),
                &user,
                json!({"area": "schematic"}),
            );
            if may(Action::WriteAnyArea) {
                assert!(result.is_ok(), "{role:?} writes: {result:?}");
                assert_eq!(status, 204, "{role:?} locks: {answer}");
                assert_eq!(
                    bench.delete(&format!("{path}/locks/schematic"), &user).0,
                    204
                );
            } else {
                assert_eq!(
                    result,
                    Err(StorageError::Forbidden),
                    "{role:?} must not write"
                );
                assert_eq!(status, 403, "{role:?} must not lock");
            }
        }
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_contributor_writes_only_assigned_areas() {
    needs_database!();
    let bench = new_bench().await;
    off(move || {
        let user = bench.member("contributor");
        let project = bench.project();
        let base = document();
        bench.push(&bench.admin, &project, &file(&base)).unwrap();
        let path = format!("/v1/projects/{project}");
        assert_eq!(
            bench
                .post(
                    &format!("{path}/grants"),
                    &bench.admin,
                    json!({"account": user.id, "area": "board"})
                )
                .0,
            204
        );
        let refused = bench.push(&user, &project, &file(&with_schematic(&base, "S")));
        assert_eq!(refused, Err(StorageError::Forbidden), "not assigned");
        assert_eq!(
            bench
                .post(
                    &format!("{path}/locks"),
                    &user,
                    json!({"area": "schematic"})
                )
                .0,
            403
        );
        bench
            .push(&user, &project, &file(&with_board(&base, 50)))
            .unwrap();
        assert_eq!(
            bench
                .post(&format!("{path}/locks"), &user, json!({"area": "board"}))
                .0,
            204
        );
        let both = with_board(&with_schematic(&base, "S"), 60);
        assert_eq!(
            bench.push(&user, &project, &file(&both)),
            Err(StorageError::Forbidden),
            "one forbidden area is enough"
        );
        assert_eq!(bench.delete(&format!("{path}/locks/board"), &user).0, 204);
        // Taken back: no more.
        assert_eq!(
            call(
                "DELETE",
                &format!("{}{path}/grants", bench.url),
                Some(&bench.admin.token),
                Some(json!({"account": user.id, "area": "board"}))
            )
            .0,
            204
        );
        assert_eq!(
            bench.push(&user, &project, &file(&with_board(&base, 70))),
            Err(StorageError::Forbidden)
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn only_those_who_assign_can_see_assignments() {
    needs_database!();
    let bench = new_bench().await;
    off(move || {
        let user = bench.member("contributor");
        let developer = bench.member("developer");
        let project = bench.project();
        let path = format!("/v1/projects/{project}/grants");
        assert_eq!(
            bench
                .post(
                    &path,
                    &bench.admin,
                    json!({"account": user.id, "area": "board"})
                )
                .0,
            204
        );
        let (status, answer) = bench.get(&path, &bench.admin);
        assert_eq!(status, 200);
        assert_eq!(answer["grants"][0]["area"], json!("board"));
        assert_eq!(bench.get(&path, &developer).0, 403);
        assert_eq!(bench.get(&path, &user).0, 403);
        assert_eq!(
            bench
                .post(
                    &path,
                    &bench.admin,
                    json!({"account": user.id, "area": "erc"})
                )
                .0,
            400
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_project_role_overrides_the_team_role_for_this_project_only() {
    needs_database!();
    let bench = new_bench().await;
    off(move || {
        let user = bench.member("viewer");
        let (one, two) = (bench.project(), bench.project());
        let base = document();
        bench.push(&bench.admin, &one, &file(&base)).unwrap();
        bench.push(&bench.admin, &two, &file(&base)).unwrap();
        assert_eq!(
            bench
                .post(
                    &format!("/v1/projects/{one}/roles"),
                    &bench.admin,
                    json!({"account": user.id, "role": "developer"})
                )
                .0,
            204
        );
        bench
            .push(&user, &one, &file(&with_board(&base, 50)))
            .unwrap();
        assert_eq!(
            bench.push(&user, &two, &file(&with_board(&base, 50))),
            Err(StorageError::Forbidden)
        );
        assert_eq!(
            bench.get(&format!("/v1/projects/{one}/access"), &user).1["role"],
            json!("developer")
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn nobody_raises_their_own_rights() {
    needs_database!();
    let bench = new_bench().await;
    off(move || {
        let team = format!("/v1/teams/{}", bench.team);
        let lead = bench.member("lead");
        let developer = bench.member("developer");
        let (_, members) = bench.get(&format!("{team}/members"), &bench.admin);
        let admin_id = members["members"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["role"] == json!("admin"))
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();

        // A lead cannot make admins or leads, invite them, or touch an admin.
        for role in ["admin", "lead"] {
            assert_eq!(
                bench
                    .patch(
                        &format!("{team}/members/{}", developer.id),
                        &lead,
                        json!({"role": role})
                    )
                    .0,
                403,
                "{role}"
            );
            assert_eq!(
                bench
                    .post(
                        &format!("{team}/invitations"),
                        &lead,
                        json!({"email": "x@example.test", "role": role})
                    )
                    .0,
                403,
                "{role}"
            );
        }
        assert_eq!(
            bench
                .patch(
                    &format!("{team}/members/{admin_id}"),
                    &lead,
                    json!({"role": "viewer"})
                )
                .0,
            403
        );
        assert_eq!(
            bench.delete(&format!("{team}/members/{admin_id}"), &lead).0,
            403
        );
        assert_eq!(
            bench
                .patch(
                    &format!("{team}/members/{}", lead.id),
                    &lead,
                    json!({"role": "admin"})
                )
                .0,
            403,
            "not even for oneself"
        );
        // What a lead may do.
        assert_eq!(
            bench
                .patch(
                    &format!("{team}/members/{}", developer.id),
                    &lead,
                    json!({"role": "viewer"})
                )
                .0,
            204
        );
        // A developer cannot do any of this.
        assert_eq!(
            bench
                .patch(
                    &format!("{team}/members/{}", developer.id),
                    &developer,
                    json!({"role": "admin"})
                )
                .0,
            403
        );
        // Nonsense roles do not exist.
        assert_eq!(
            bench
                .patch(
                    &format!("{team}/members/{}", developer.id),
                    &bench.admin,
                    json!({"role": "owner"})
                )
                .0,
            400
        );
        // A project role cannot go past the same limit.
        let project = bench.project();
        assert_eq!(
            bench
                .post(
                    &format!("/v1/projects/{project}/roles"),
                    &lead,
                    json!({"account": developer.id, "role": "admin"})
                )
                .0,
            403
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_team_always_keeps_an_administrator() {
    needs_database!();
    let bench = new_bench().await;
    off(move || {
        let team = format!("/v1/teams/{}", bench.team);
        let (_, members) = bench.get(&format!("{team}/members"), &bench.admin);
        let admin_id = members["members"][0]["id"].as_str().unwrap().to_owned();
        assert_eq!(
            bench
                .patch(
                    &format!("{team}/members/{admin_id}"),
                    &bench.admin,
                    json!({"role": "viewer"})
                )
                .0,
            409
        );
        assert_eq!(
            bench
                .delete(&format!("{team}/members/{admin_id}"), &bench.admin)
                .0,
            409
        );
        let other = bench.member("viewer");
        assert_eq!(
            call(
                "DELETE",
                &format!("{}/v1/account", bench.url),
                Some(&bench.admin.token),
                Some(json!({"password": PASSWORD}))
            )
            .0,
            409,
            "the only administrator cannot simply leave with an account deletion"
        );
        assert_eq!(
            bench
                .patch(
                    &format!("{team}/members/{}", other.id),
                    &bench.admin,
                    json!({"role": "admin"})
                )
                .0,
            204
        );
        assert_eq!(
            bench
                .patch(
                    &format!("{team}/members/{admin_id}"),
                    &bench.admin,
                    json!({"role": "viewer"})
                )
                .0,
            204
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn outsiders_and_former_members_get_nothing() {
    needs_database!();
    let bench = new_bench().await;
    off(move || {
        let team = format!("/v1/teams/{}", bench.team);
        let project = bench.project();
        let base = document();
        bench.push(&bench.admin, &project, &file(&base)).unwrap();
        let stranger = bench.new_account();
        let member = bench.member("developer");
        assert_eq!(
            bench
                .delete(&format!("{team}/members/{}", member.id), &bench.admin)
                .0,
            204
        );
        for user in [&stranger, &member] {
            assert_eq!(bench.get(&format!("{team}/members"), user).0, 403);
            assert_eq!(bench.get(&format!("{team}/audit"), user).0, 403);
            assert_eq!(
                bench.get(&format!("/v1/projects/{project}/access"), user).0,
                403
            );
            assert_eq!(
                bench
                    .get(&format!("/v1/projects/{project}/comments"), user)
                    .0,
                403
            );
            assert_eq!(
                bench
                    .post(
                        &format!("/v1/projects/{project}/locks"),
                        user,
                        json!({"area": "board"})
                    )
                    .0,
                403
            );
            assert_eq!(
                bench
                    .provider(user)
                    .list(&format!("projects/{project}/commits")),
                Err(StorageError::Forbidden)
            );
            assert_eq!(
                bench.push(user, &project, &file(&with_board(&base, 50))),
                Err(StorageError::Forbidden)
            );
        }
        assert_eq!(
            call("GET", &format!("{}{team}/members", bench.url), None, None).0,
            401
        );
        // Other teams' projects cannot be taken over.
        let (_, other) = bench.post("/v1/teams", &stranger, json!({"name": "Other"}));
        let taken = bench.post(
            &format!("/v1/teams/{}/projects", other["id"].as_str().unwrap()),
            &stranger,
            json!({"repo": project, "name": "Mine"}),
        );
        assert_eq!(taken.0, 403);
        // Team projects are not shared by the old route either.
        assert_eq!(
            bench
                .post(
                    &format!("/v1/repos/{project}/shares"),
                    &bench.admin,
                    json!({"email": stranger.email, "can_write": true})
                )
                .0,
            409
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn invitations_work_once_and_only_for_their_address() {
    needs_database!();
    let bench = new_bench().await;
    off(move || {
        let team = format!("/v1/teams/{}", bench.team);
        let invited = bench.new_account();
        let other = bench.new_account();
        let (_, answer) = bench.post(
            &format!("{team}/invitations"),
            &bench.admin,
            json!({"email": invited.email, "role": "developer"}),
        );
        let code = answer["code"].clone();
        assert_eq!(
            bench
                .post("/v1/invitations/accept", &other, json!({"code": code}))
                .0,
            400,
            "another address"
        );
        assert_eq!(
            bench
                .post("/v1/invitations/accept", &invited, json!({"code": "wrong"}))
                .0,
            400
        );
        assert_eq!(
            bench
                .post("/v1/invitations/accept", &invited, json!({"code": code}))
                .0,
            200
        );
        assert_eq!(
            bench
                .post("/v1/invitations/accept", &invited, json!({"code": code}))
                .0,
            400,
            "only once"
        );
        // Revoked.
        let (_, answer) = bench.post(
            &format!("{team}/invitations"),
            &bench.admin,
            json!({"email": other.email, "role": "viewer"}),
        );
        assert_eq!(
            call(
                "DELETE",
                &format!("{}{team}/invitations", bench.url),
                Some(&bench.admin.token),
                Some(json!({"email": other.email}))
            )
            .0,
            204
        );
        assert_eq!(
            bench
                .post(
                    "/v1/invitations/accept",
                    &other,
                    json!({"code": answer["code"]})
                )
                .0,
            400
        );
    })
    .await;
}

/// A closed server lets in only those with an invitation.
#[tokio::test(flavor = "multi_thread")]
async fn closed_registration_needs_an_invitation() {
    needs_database!();
    let open = new_bench().await;
    let closed = instance_on(&database().unwrap(), 1 << 32, false).await;
    off(move || {
        let register = format!("{closed}/v1/auth/register");
        let email = format!("new-{}@example.test", unique());
        assert_eq!(
            call(
                "POST",
                &register,
                None,
                Some(json!({"email": email, "password": PASSWORD}))
            )
            .0,
            403
        );
        assert_eq!(
            call(
                "POST",
                &register,
                None,
                Some(json!({"email": email, "password": PASSWORD, "invitation": "wrong"}))
            )
            .0,
            403
        );
        let (_, answer) = open.post(
            &format!("/v1/teams/{}/invitations", open.team),
            &open.admin,
            json!({"email": email, "role": "viewer"}),
        );
        let other = format!("other-{}@example.test", unique());
        assert_eq!(
            call(
                "POST",
                &register,
                None,
                Some(json!({"email": other, "password": PASSWORD, "invitation": answer["code"]}))
            )
            .0,
            403,
            "not for another address"
        );
        assert_eq!(
            call(
                "POST",
                &register,
                None,
                Some(json!({"email": email, "password": PASSWORD, "invitation": answer["code"]}))
            )
            .0,
            201
        );
    })
    .await;
}

// ---- Areas and locks ----

#[tokio::test(flavor = "multi_thread")]
async fn a_lock_stops_others_by_what_they_really_change() {
    needs_database!();
    let bench = new_bench().await;
    off(move || {
        let a = bench.member("developer");
        let b = bench.member("developer");
        let project = bench.project();
        let base = document();
        bench.push(&bench.admin, &project, &file(&base)).unwrap();
        let locks = format!("/v1/projects/{project}/locks");
        assert_eq!(bench.post(&locks, &a, json!({"area": "schematic"})).0, 204);

        // B is stopped in the locked area, with the reason.
        let refused = bench.push(&b, &project, &file(&with_schematic(&base, "B")));
        match refused {
            Err(StorageError::Server {
                status: 423,
                message,
            }) => {
                assert!(
                    message.contains("schematic") && message.contains(&a.email),
                    "{message}"
                );
            }
            other => panic!("{other:?}"),
        }
        // Also when the locked change is hidden among others.
        let mixed = with_board(&with_schematic(&base, "B"), 50);
        assert!(matches!(
            bench.push(&b, &project, &file(&mixed)),
            Err(StorageError::Server { status: 423, .. })
        ));
        // A second person cannot take the lock.
        let (status, answer) = bench.post(&locks, &b, json!({"area": "schematic"}));
        assert_eq!(status, 423);
        assert_eq!(answer["holder"], json!(a.email));

        // Other areas stay free, and the holder works on.
        bench
            .push(&b, &project, &file(&with_board(&base, 50)))
            .unwrap();
        let current = decode(&bench.newest(&project), Path::new("x")).unwrap();
        bench
            .push(&a, &project, &file(&with_schematic(&current, "A")))
            .unwrap();
        // Released: free for all.
        assert_eq!(bench.delete(&format!("{locks}/schematic"), &a).0, 204);
        let current = decode(&bench.newest(&project), Path::new("x")).unwrap();
        bench
            .push(&b, &project, &file(&with_schematic(&current, "B2")))
            .unwrap();
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn only_one_of_many_gets_a_lock() {
    needs_database!();
    let bench = new_bench().await;
    let project = {
        let bench = bench.clone();
        off(move || bench.project()).await
    };
    let users: Vec<User> = {
        let bench = bench.clone();
        off(move || (0..8).map(|_| bench.member("developer")).collect()).await
    };
    let mut tasks = Vec::new();
    for user in users {
        let (bench, project) = (bench.clone(), project.clone());
        tasks.push(tokio::task::spawn_blocking(move || {
            bench
                .post(
                    &format!("/v1/projects/{project}/locks"),
                    &user,
                    json!({"area": "board"}),
                )
                .0
        }));
    }
    let mut statuses = Vec::new();
    for task in tasks {
        statuses.push(task.await.unwrap());
    }
    assert_eq!(
        statuses.iter().filter(|s| **s == 204).count(),
        1,
        "{statuses:?}"
    );
    assert_eq!(
        statuses.iter().filter(|s| **s == 423).count(),
        7,
        "{statuses:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lock_can_be_ended_only_by_its_holder_or_a_lead_and_expires() {
    needs_database!();
    let bench = new_bench().await;
    off(move || {
        let holder = bench.member("developer");
        let other = bench.member("developer");
        let lead = bench.member("lead");
        let project = bench.project();
        let locks = format!("/v1/projects/{project}/locks");
        assert_eq!(bench.post(&locks, &holder, json!({"area": "board"})).0, 204);
        assert_eq!(bench.delete(&format!("{locks}/board"), &other).0, 403);
        assert_eq!(
            bench.get(&locks, &other).1["locks"][0]["email"],
            json!(holder.email)
        );
        assert_eq!(
            bench.delete(&format!("{locks}/board"), &lead).0,
            204,
            "someone away must not block all"
        );
        let (_, notes) = bench.get("/v1/notifications", &holder);
        assert!(
            notes["notifications"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n["kind"] == json!("lock_overridden")),
            "the holder learns about it"
        );
        // Removing a member frees their locks.
        assert_eq!(bench.post(&locks, &other, json!({"area": "board"})).0, 204);
        assert_eq!(
            bench
                .delete(
                    &format!("/v1/teams/{}/members/{}", bench.team, other.id),
                    &bench.admin
                )
                .0,
            204
        );
        assert_eq!(bench.post(&locks, &holder, json!({"area": "board"})).0, 204);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_expired_lock_stops_nobody() {
    needs_database!();
    let bench = new_bench().await;
    let (a, b, project) = {
        let bench = bench.clone();
        off(move || {
            let a = bench.member("developer");
            let b = bench.member("developer");
            let project = bench.project();
            bench
                .push(&bench.admin, &project, &file(&document()))
                .unwrap();
            assert_eq!(
                bench
                    .post(
                        &format!("/v1/projects/{project}/locks"),
                        &a,
                        json!({"area": "board"})
                    )
                    .0,
                204
            );
            (a, b, project)
        })
        .await
    };
    let (db, connection) = tokio_postgres::connect(&database().unwrap(), tokio_postgres::NoTls)
        .await
        .unwrap();
    tokio::spawn(connection);
    let id: uuid::Uuid = project.parse().unwrap();
    db.execute(
        "UPDATE locks SET expires = now() - interval '1 second' WHERE repo = $1",
        &[&id],
    )
    .await
    .unwrap();
    off(move || {
        let current = decode(&bench.newest(&project), Path::new("x")).unwrap();
        bench
            .push(&b, &project, &file(&with_board(&current, 90)))
            .unwrap();
        assert_eq!(
            bench
                .post(
                    &format!("/v1/projects/{project}/locks"),
                    &b,
                    json!({"area": "board"})
                )
                .0,
            204
        );
        let _ = a;
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn commits_cannot_lie_about_their_content() {
    needs_database!();
    let bench = new_bench().await;
    off(move || {
        let holder = bench.member("developer");
        let other = bench.member("developer");
        let project = bench.project();
        let base = document();
        let head = bench.push(&bench.admin, &project, &file(&base)).unwrap();
        assert_eq!(
            bench
                .post(
                    &format!("/v1/projects/{project}/locks"),
                    &holder,
                    json!({"area": "schematic"})
                )
                .0,
            204
        );
        let provider = bench.provider(&other);
        let commit_for = |parents: Vec<String>, blob: String| {
            let bytes = serde_json::to_vec(&Commit {
                parents,
                blob,
                size: 1,
                device: "x".to_owned(),
                created: "2026-01-01T00:00:00Z".to_owned(),
            })
            .unwrap();
            let key = format!("projects/{project}/commits/{}.json", sha256_hex(&bytes));
            (key, bytes)
        };
        // A blob that is no project file.
        let garbage = b"not a project".to_vec();
        provider
            .put(
                &format!("projects/{project}/blobs/{}", sha256_hex(&garbage)),
                &garbage,
            )
            .unwrap();
        let (key, bytes) = commit_for(vec![head.clone()], sha256_hex(&garbage));
        assert!(matches!(
            provider.put(&key, &bytes),
            Err(StorageError::Server { status: 400, .. })
        ));
        // A commit naming content that was never uploaded.
        let (key, bytes) = commit_for(vec![head.clone()], "0".repeat(64));
        assert!(matches!(
            provider.put(&key, &bytes),
            Err(StorageError::Server { status: 400, .. })
        ));
        // A forged parent that does not exist.
        let changed = file(&with_schematic(&base, "Sneaky"));
        provider
            .put(
                &format!("projects/{project}/blobs/{}", sha256_hex(&changed)),
                &changed,
            )
            .unwrap();
        let (key, bytes) = commit_for(vec!["f".repeat(64)], sha256_hex(&changed));
        assert!(matches!(
            provider.put(&key, &bytes),
            Err(StorageError::Server { status: 400, .. })
        ));
        // No parents at all, to start a second history past the lock.
        let (key, bytes) = commit_for(Vec::new(), sha256_hex(&changed));
        assert!(matches!(
            provider.put(&key, &bytes),
            Err(StorageError::Server { status: 423, .. })
        ));
        // Claiming the parent it really changed: stopped by the content.
        let (key, bytes) = commit_for(vec![head], sha256_hex(&changed));
        assert!(matches!(
            provider.put(&key, &bytes),
            Err(StorageError::Server { status: 423, .. })
        ));
    })
    .await;
}

/// The example from the assignment: one person on the schematic, one on the board,
/// one on the mechanics, at the same time. All of it arrives, and what does not fit
/// together between schematic and board is reported.
/// Found by the quality gate: every start replaced the audit triggers, and a
/// request on a running instance could deadlock against that.
#[tokio::test(flavor = "multi_thread")]
async fn starting_an_instance_does_not_disturb_the_running_ones() {
    needs_database!();
    let bench = new_bench().await;
    let (client, connection) = tokio_postgres::connect(&database().unwrap(), tokio_postgres::NoTls)
        .await
        .unwrap();
    tokio::spawn(connection);
    // A deadlock only happens now and then. Touching the tables at all shows every time.
    let triggers = "SELECT oid::bigint FROM pg_trigger WHERE tgname = 'audit_no_change'";
    let before: i64 = client.query_one(triggers, &[]).await.unwrap().get(0);
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stopper = stop.clone();
    let traffic = tokio::task::spawn_blocking(move || {
        let mut requests = 0;
        while !stopper.load(Ordering::Relaxed) {
            // Writes the tables that have triggers and foreign keys, audit first of all.
            let (status, answer) =
                bench.post("/v1/teams", &bench.admin, json!({"name": "Under way"}));
            assert_eq!(status, 201, "{answer}");
            requests += 1;
            // Each request is a new connection, and Windows runs out of ports otherwise.
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        requests
    });
    for _ in 0..12 {
        instance().await;
    }
    stop.store(true, Ordering::Relaxed);
    assert!(traffic.await.unwrap() > 0);
    let after: i64 = client.query_one(triggers, &[]).await.unwrap().get(0);
    assert_eq!(before, after, "a start changed the tables");
}

/// A new version does change the schema while others work. That must still start.
#[tokio::test(flavor = "multi_thread")]
async fn a_changed_schema_is_applied_while_others_work() {
    needs_database!();
    let shared = database().unwrap();
    let (base, _) = shared.rsplit_once('/').unwrap();
    let name = format!("tronic_schema_{}", unique().replace('-', "_"));
    let own = format!("{base}/{name}");
    let open = |url: String| async move {
        let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls)
            .await
            .unwrap();
        tokio::spawn(connection);
        client
    };
    let admin = open(shared).await;
    admin
        .batch_execute(&format!("CREATE DATABASE {name}"))
        .await
        .unwrap();

    let url = instance_on(&own, 1 << 32, true).await;
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stopper = stop.clone();
    let traffic = tokio::task::spawn_blocking(move || {
        let email = format!("admin-{}@example.test", unique());
        let token = CloudProvider::register(&url, &email, PASSWORD).unwrap();
        while !stopper.load(Ordering::Relaxed) {
            // Here a request may lose against the change, so only starting counts.
            call(
                "POST",
                &format!("{url}/v1/teams"),
                Some(&token),
                Some(json!({"name": "Under way"})),
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    });
    let inside = open(own.clone()).await;
    for _ in 0..8 {
        inside
            .batch_execute("DELETE FROM schema_applied")
            .await
            .unwrap();
        instance_on(&own, 1 << 32, true).await;
        let applied = inside
            .query_one("SELECT count(*) FROM schema_applied", &[])
            .await
            .unwrap();
        assert_eq!(applied.get::<_, i64>(0), 1);
    }
    stop.store(true, Ordering::Relaxed);
    traffic.await.unwrap();
    // The triggers survived being replaced.
    assert!(inside.batch_execute("DELETE FROM audit").await.is_err());
    admin
        .batch_execute(&format!("DROP DATABASE {name} WITH (FORCE)"))
        .await
        .unwrap();
}

/// Every small request is read completely before the answer, also one that needs no
/// body, and one that is too large gets a clear answer.
#[tokio::test(flavor = "multi_thread")]
async fn requests_are_read_before_they_are_answered() {
    needs_database!();
    let url = instance().await;
    off(move || {
        for _ in 0..50 {
            let (status, _) = call(
                "POST",
                &format!("{url}/v1/auth/logout"),
                None,
                Some(json!({"filler": "x".repeat(30_000)})),
            );
            assert_eq!(status, 401);
        }
        let (status, _) = call(
            "POST",
            &format!("{url}/v1/auth/logout"),
            None,
            Some(json!({"filler": "x".repeat(70_000)})),
        );
        assert_eq!(status, 413);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn three_people_in_three_areas_come_together() {
    needs_database!();
    let bench = new_bench().await;
    off(move || {
        let (a, b, c) = (
            bench.member("developer"),
            bench.member("developer"),
            bench.member("developer"),
        );
        let project = bench.project();
        let base = document();
        let base_bytes = file(&base);
        bench.push(&bench.admin, &project, &base_bytes).unwrap();
        for (user, area) in [(&a, "schematic"), (&b, "board"), (&c, "mechanics")] {
            assert_eq!(
                bench
                    .post(
                        &format!("/v1/projects/{project}/locks"),
                        user,
                        json!({"area": area})
                    )
                    .0,
                204
            );
        }
        // Each starts from the same state and stores their part.
        let edit_a = file(&with_schematic(&base, "From A"));
        let edit_b = file(&with_board(&base, 42));
        let edit_c = file(&with_mechanics(&base, 800_000));
        bench.push(&a, &project, &edit_a).unwrap();

        // B and C merge what is there with their own, as the application does.
        let merged_b = match merge(&base_bytes, &edit_b, &bench.newest(&project)).unwrap() {
            Merge::Merged { bytes, .. } => bytes,
            other @ Merge::Conflict { .. } => panic!("{other:?}"),
        };
        bench.push(&b, &project, &merged_b).unwrap();
        let merged_c = match merge(&base_bytes, &edit_c, &bench.newest(&project)).unwrap() {
            Merge::Merged { bytes, .. } => bytes,
            other @ Merge::Conflict { .. } => panic!("{other:?}"),
        };
        bench.push(&c, &project, &merged_c).unwrap();

        let result = decode(&bench.newest(&project), Path::new("x")).unwrap();
        assert_eq!(result.schematic.sheets[0].name, "From A");
        assert_eq!(result.board.outline.len(), 3);
        assert_eq!(result.board.mechanics.thickness, 800_000);
    })
    .await;
}

// ---- Comments, notifications, presence ----

#[tokio::test(flavor = "multi_thread")]
async fn comments_notifications_and_presence() {
    needs_database!();
    let bench = new_bench().await;
    off(move || {
        let a = bench.member("developer");
        let b = bench.member("contributor");
        let project = bench.project();
        let path = format!("/v1/projects/{project}");
        let (status, comment) = bench.post(
            &format!("{path}/comments"),
            &a,
            json!({"area": "schematic", "anchor": "R1", "body": "Zu klein?"}),
        );
        assert_eq!(status, 201);
        assert_eq!(
            bench.get(&format!("{path}/comments"), &b).1["comments"][0]["body"],
            json!("Zu klein?")
        );
        let id = comment["id"].as_str().unwrap();
        assert_eq!(
            bench
                .post(&format!("{path}/comments/{id}/resolve"), &b, json!({}))
                .0,
            403,
            "not someone else's"
        );
        assert_eq!(
            bench
                .post(&format!("{path}/comments/{id}/resolve"), &a, json!({}))
                .0,
            204
        );
        assert_eq!(
            bench.get(&format!("{path}/comments"), &b).1["comments"][0]["resolved"],
            json!(true)
        );
        assert_eq!(
            bench
                .post(
                    &format!("{path}/comments"),
                    &a,
                    json!({"area": "nonsense", "anchor": "", "body": "x"})
                )
                .0,
            400
        );
        assert_eq!(
            bench
                .post(
                    &format!("{path}/comments"),
                    &a,
                    json!({"area": "board", "anchor": "", "body": "  "})
                )
                .0,
            400
        );

        let (_, notes) = bench.get("/v1/notifications", &b);
        let list = notes["notifications"].as_array().unwrap();
        assert!(list
            .iter()
            .any(|n| n["kind"] == json!("comment") && n["read"] == json!(false)));
        assert_eq!(bench.post("/v1/notifications/read", &b, json!({})).0, 204);
        let (_, notes) = bench.get("/v1/notifications", &b);
        assert!(notes["notifications"]
            .as_array()
            .unwrap()
            .iter()
            .all(|n| n["read"] == json!(true)));

        assert_eq!(
            bench
                .post(&format!("{path}/presence"), &a, json!({"area": "board"}))
                .0,
            204
        );
        let (_, here) = bench.get(&format!("{path}/presence"), &b);
        assert_eq!(here["presence"][0]["email"], json!(a.email));
        assert_eq!(here["presence"][0]["area"], json!("board"));
        let (_, members) = bench.get(&format!("/v1/teams/{}/members", bench.team), &b);
        let online = members["members"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["email"] == json!(a.email))
            .unwrap();
        assert_eq!(online["online"], json!(true));
    })
    .await;
}

// ---- Audit ----

/// Every kind of change, once. Each must appear in the log, with who did it.
#[tokio::test(flavor = "multi_thread")]
async fn the_audit_log_holds_every_change() {
    needs_database!();
    let bench = new_bench().await;
    let db_url = database().unwrap();
    let (team, project, admin_email, member_email) = {
        let bench = bench.clone();
        off(move || {
            let team = format!("/v1/teams/{}", bench.team);
            let project = bench.project();
            let path = format!("/v1/projects/{project}");
            let member = bench.member("developer");
            let base = document();
            bench.push(&bench.admin, &project, &file(&base)).unwrap();
            bench.patch(&team, &bench.admin, json!({"name": "New name"}));
            bench.patch(
                &format!("{team}/members/{}", member.id),
                &bench.admin,
                json!({"role": "contributor"}),
            );
            let (_, invitation) = bench.post(
                &format!("{team}/invitations"),
                &bench.admin,
                json!({"email": "gone@example.test", "role": "viewer"}),
            );
            assert!(invitation["code"].is_string());
            call(
                "DELETE",
                &format!("{}{team}/invitations", bench.url),
                Some(&bench.admin.token),
                Some(json!({"email": "gone@example.test"})),
            );
            bench.post(
                &format!("{path}/roles"),
                &bench.admin,
                json!({"account": member.id, "role": "developer"}),
            );
            bench.post(
                &format!("{path}/grants"),
                &bench.admin,
                json!({"account": member.id, "area": "board"}),
            );
            call(
                "DELETE",
                &format!("{}{path}/grants", bench.url),
                Some(&bench.admin.token),
                Some(json!({"account": member.id, "area": "board"})),
            );
            bench.post(&format!("{path}/locks"), &member, json!({"area": "board"}));
            bench.delete(&format!("{path}/locks/board"), &member);
            bench.post(&format!("{path}/locks"), &member, json!({"area": "board"}));
            bench.delete(&format!("{path}/locks/board"), &bench.admin);
            let (_, comment) = bench.post(
                &format!("{path}/comments"),
                &member,
                json!({"area": "board", "anchor": "", "body": "x"}),
            );
            bench.post(
                &format!(
                    "{path}/comments/{}/resolve",
                    comment["id"].as_str().unwrap()
                ),
                &member,
                json!({}),
            );
            let leaving = bench.member("viewer");
            bench.delete(&format!("{team}/members/{}", leaving.id), &bench.admin);
            (team, project, bench.admin.email.clone(), member.email)
        })
        .await
    };
    let audit_bench = bench.clone();
    let audit_team = team.clone();
    let (status, answer) =
        off(move || audit_bench.get(&format!("{audit_team}/audit"), &audit_bench.admin)).await;
    assert_eq!(status, 200);
    assert_eq!(answer["first_damaged"], Value::Null, "the chain is whole");
    let entries = answer["entries"].as_array().unwrap();
    let actions: BTreeSet<&str> = entries
        .iter()
        .map(|e| e["action"].as_str().unwrap())
        .collect();
    for expected in [
        "team.create",
        "team.rename",
        "member.role",
        "member.remove",
        "invitation.create",
        "invitation.revoke",
        "invitation.accept",
        "project.add",
        "project.role",
        "area.grant",
        "area.revoke",
        "lock.take",
        "lock.release",
        "lock.override",
        "comment.add",
        "comment.resolve",
        "version.add",
    ] {
        assert!(
            actions.contains(expected),
            "{expected} missing in {actions:?}"
        );
    }
    let of = |action: &str| {
        entries
            .iter()
            .find(|e| e["action"] == json!(action))
            .unwrap()
    };
    assert_eq!(of("lock.override")["actor_email"], json!(admin_email));
    assert_eq!(of("lock.release")["actor_email"], json!(member_email));
    assert_eq!(of("version.add")["repo"], json!(project));
    assert_eq!(
        of("version.add")["detail"],
        json!("metadata,schematic,board,mechanics")
    );

    // Outside teams too.
    let (db, connection) = tokio_postgres::connect(&db_url, tokio_postgres::NoTls)
        .await
        .unwrap();
    tokio::spawn(connection);
    let email = bench.admin.email.clone();
    let personal: Vec<String> = db
        .query("SELECT action FROM audit WHERE actor_email = $1", &[&email])
        .await
        .unwrap()
        .iter()
        .map(|row| row.get(0))
        .collect();
    assert!(
        personal.contains(&"account.create".to_owned()),
        "{personal:?}"
    );
    let login_url = bench.url.clone();
    let wrong = email.clone();
    off(move || {
        call(
            "POST",
            &format!("{login_url}/v1/auth/login"),
            None,
            Some(json!({"email": wrong, "password": "the wrong password"})),
        );
        call(
            "POST",
            &format!("{login_url}/v1/auth/login"),
            None,
            Some(json!({"email": wrong, "password": PASSWORD})),
        );
    })
    .await;
    let personal: Vec<String> = db
        .query("SELECT action FROM audit WHERE actor_email = $1", &[&email])
        .await
        .unwrap()
        .iter()
        .map(|row| row.get(0))
        .collect();
    assert!(personal.contains(&"login.failure".to_owned()));
    assert!(personal.contains(&"login.success".to_owned()));

    // The database refuses changes to it.
    assert!(db
        .execute(
            "UPDATE audit SET detail = 'x' WHERE seq = (SELECT max(seq) FROM audit)",
            &[]
        )
        .await
        .is_err());
    assert!(db
        .execute(
            "DELETE FROM audit WHERE seq = (SELECT max(seq) FROM audit)",
            &[]
        )
        .await
        .is_err());
    assert!(db.batch_execute("TRUNCATE audit").await.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn only_those_allowed_read_the_log_and_only_their_team() {
    needs_database!();
    let bench = new_bench().await;
    let other = new_bench().await;
    off(move || {
        let (_, answer) = bench.get(&format!("/v1/teams/{}/audit", bench.team), &bench.admin);
        let entries = answer["entries"].as_array().unwrap();
        assert!(!entries.is_empty());
        assert!(
            entries.iter().all(|e| e["team"] == json!(bench.team)),
            "no other team's entries"
        );
        assert_eq!(
            bench
                .get(&format!("/v1/teams/{}/audit", other.team), &bench.admin)
                .0,
            403
        );
    })
    .await;
}

/// Someone with full database access can still change the log, but it shows. This
/// damages the chain, so it runs in a database of its own.
#[tokio::test(flavor = "multi_thread")]
async fn tampering_with_the_log_is_noticed() {
    needs_database!();
    let url = database().unwrap();
    let (admin, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls)
        .await
        .unwrap();
    tokio::spawn(connection);
    let name = format!("tronic_tamper_{}", unique().replace('-', "_"));
    if admin
        .batch_execute(&format!("CREATE DATABASE {name}"))
        .await
        .is_err()
    {
        eprintln!("skipped: the test role may not create databases");
        return;
    }
    let separate = url
        .rsplit_once('/')
        .map(|(base, _)| format!("{base}/{name}"))
        .unwrap();
    let address = instance_on(&separate, 1 << 30, true).await;
    let (team_url, token) = (address.clone(), {
        let address = address.clone();
        off(move || CloudProvider::register(&address, "tamper@example.test", PASSWORD).unwrap())
            .await
    });
    let (team, first) = off(move || {
        let (_, team) = call(
            "POST",
            &format!("{team_url}/v1/teams"),
            Some(&token),
            Some(json!({"name": "T"})),
        );
        let id = team["id"].as_str().unwrap().to_owned();
        let audit = format!("{team_url}/v1/teams/{id}/audit");
        let (_, before) = call("GET", &audit, Some(&token), None);
        (audit, (before["first_damaged"].clone(), token))
    })
    .await;
    assert_eq!(first.0, Value::Null);
    let (db, connection) = tokio_postgres::connect(&separate, tokio_postgres::NoTls)
        .await
        .unwrap();
    tokio::spawn(connection);
    db.batch_execute(
        "ALTER TABLE audit DISABLE TRIGGER audit_no_change;
         UPDATE audit SET detail = 'forged' WHERE action = 'team.create';
         ALTER TABLE audit ENABLE TRIGGER audit_no_change;",
    )
    .await
    .unwrap();
    let token = first.1;
    let (_, after) = off(move || call("GET", &team, Some(&token), None)).await;
    assert!(after["first_damaged"].is_i64(), "{after}");
}
