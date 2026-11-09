import { FormEvent, useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { backend, ProjectView } from "../api/backend";
import { errorMessage, formatDate } from "../components/format";
import { Target } from "../cloud/types";

const AREAS = ["schematic", "board", "mechanics", "metadata"] as const;
const ROLES = ["admin", "lead", "developer", "contributor", "viewer"] as const;
const PRESENCE_MS = 30_000;

interface Team {
  id: string;
  name: string;
  role: string;
}
interface Member {
  id: string;
  email: string;
  role: string;
  online: boolean;
}
interface TeamProject {
  id: string;
  name: string;
}
interface Lock {
  area: string;
  email: string;
  expires: string;
  mine: boolean;
}
interface Comment {
  id: string;
  author: string;
  area: string;
  anchor: string;
  body: string;
  created: string;
  resolved: boolean;
}
interface Note {
  id: number;
  kind: string;
  params: Record<string, string>;
  created: string;
  read: boolean;
}
interface AuditEntry {
  seq: number;
  at: string;
  actor_email: string;
  action: string;
  detail: string;
}
interface Access {
  role: string;
  can: string[];
  areas: string[];
}

export function TeamPanel(props: {
  project: ProjectView;
  onProject: (view: ProjectView) => void;
  guarded: (action: () => void | Promise<void>) => void;
}) {
  const { t, i18n } = useTranslation();
  const { project, onProject, guarded } = props;
  const [targets, setTargets] = useState<Target[]>([]);
  const [target, setTarget] = useState("");
  const [teams, setTeams] = useState<Team[]>([]);
  const [team, setTeam] = useState("");
  const [members, setMembers] = useState<Member[]>([]);
  const [projects, setProjects] = useState<TeamProject[]>([]);
  const [access, setAccess] = useState<Access | null>(null);
  const [locks, setLocks] = useState<Lock[]>([]);
  const [grants, setGrants] = useState<{ account: string; area: string }[]>([]);
  const [here, setHere] = useState<{ email: string; area: string }[]>([]);
  const [comments, setComments] = useState<Comment[]>([]);
  const [notes, setNotes] = useState<Note[]>([]);
  const [audit, setAudit] = useState<{ entries: AuditEntry[]; first_damaged: number | null } | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [code, setCode] = useState<string | null>(null);
  const [form, setForm] = useState({ teamName: "", joinCode: "", email: "", role: "developer", comment: "", anchor: "", area: "schematic", working: "schematic" });
  const set = (key: keyof typeof form) => (value: string) => setForm((current) => ({ ...current, [key]: value }));

  const call = useCallback(
    <T,>(method: string, path: string, body?: unknown) => backend.team.call<T>(target, method, path, body ?? null),
    [target],
  );

  const run = useCallback(
    async (action: () => Promise<void>) => {
      setFailure(null);
      setNotice(null);
      try {
        await action();
      } catch (error) {
        setFailure(errorMessage(t, error));
      }
    },
    [t],
  );

  // Loading in the background keeps what the user was just told.
  const quietly = useCallback(
    (action: () => Promise<void>) => {
      action().catch((error) => setFailure(errorMessage(t, error)));
    },
    [t],
  );

  const loadTeams = useCallback(async () => {
    const answer = await call<{ teams: Team[] }>("GET", "/v1/teams");
    setTeams(answer.teams);
    setTeam((current) => (answer.teams.some((item) => item.id === current) ? current : (answer.teams[0]?.id ?? "")));
    const list = await call<{ notifications: Note[] }>("GET", "/v1/notifications");
    setNotes(list.notifications);
  }, [call]);

  const loadTeam = useCallback(async () => {
    const answer = await call<{ members: Member[] }>("GET", `/v1/teams/${team}/members`);
    setMembers(answer.members);
    const list = await call<{ projects: TeamProject[] }>("GET", `/v1/teams/${team}/projects`);
    setProjects(list.projects);
  }, [call, team]);

  const inThisTeam = projects.some((item) => item.id === project.id);

  const loadProject = useCallback(async () => {
    const path = `/v1/projects/${project.id}`;
    const mine = await call<Access>("GET", `${path}/access`);
    setAccess(mine);
    setGrants(mine.can.includes("manage_project") ? (await call<{ grants: { account: string; area: string }[] }>("GET", `${path}/grants`)).grants : []);
    setLocks((await call<{ locks: Lock[] }>("GET", `${path}/locks`)).locks);
    setHere((await call<{ presence: { email: string; area: string }[] }>("GET", `${path}/presence`)).presence);
    setComments((await call<{ comments: Comment[] }>("GET", `${path}/comments`)).comments);
  }, [call, project.id]);

  useEffect(() => {
    backend.cloud
      .overview()
      .then((overview) => {
        const clouds = overview.targets.filter((item) => item.kind === "cloud");
        setTargets(clouds);
        setTarget((current) => current || (clouds[0]?.id ?? ""));
      })
      .catch((error) => setFailure(errorMessage(t, error)));
  }, [t]);

  useEffect(() => {
    if (target) quietly(loadTeams);
  }, [target, loadTeams, quietly]);

  useEffect(() => {
    setAudit(null);
    if (team) quietly(loadTeam);
  }, [team, loadTeam, quietly]);

  useEffect(() => {
    setAccess(null);
    if (inThisTeam) quietly(loadProject);
  }, [inThisTeam, loadProject, quietly]);

  // Who is working on what. A person counts as present while this panel is open.
  useEffect(() => {
    if (!inThisTeam) return;
    const send = () => {
      void call("POST", `/v1/projects/${project.id}/presence`, { area: form.working }).catch(() => undefined);
    };
    send();
    const timer = window.setInterval(send, PRESENCE_MS);
    return () => window.clearInterval(timer);
  }, [inThisTeam, call, project.id, form.working]);

  const may = (action: string) => access?.can.includes(action) ?? false;
  const myRole = teams.find((item) => item.id === team)?.role;
  const canManage = myRole === "admin" || myRole === "lead";
  const canAudit = canManage;

  const createTeam = (event: FormEvent) => {
    event.preventDefault();
    void run(async () => {
      await call("POST", "/v1/teams", { name: form.teamName });
      set("teamName")("");
      await loadTeams();
    });
  };

  const join = (event: FormEvent) => {
    event.preventDefault();
    void run(async () => {
      await call("POST", "/v1/invitations/accept", { code: form.joinCode.trim() });
      set("joinCode")("");
      await loadTeams();
      setNotice(t("team.joined"));
    });
  };

  const invite = (event: FormEvent) => {
    event.preventDefault();
    void run(async () => {
      const answer = await call<{ code: string }>("POST", `/v1/teams/${team}/invitations`, { email: form.email, role: form.role });
      setCode(answer.code);
      set("email")("");
    });
  };

  const addComment = (event: FormEvent) => {
    event.preventDefault();
    void run(async () => {
      await call("POST", `/v1/projects/${project.id}/comments`, { area: form.area, anchor: form.anchor, body: form.comment });
      set("comment")("");
      set("anchor")("");
      await loadProject();
    });
  };

  const clone = (item: TeamProject) =>
    guarded(async () => {
      await run(async () => {
        const path = await backend.pickSavePath(t("fileFilter.project"), `${item.name}.tronic`);
        if (path) onProject(await backend.team.clone(target, item.id, path));
      });
    });

  const attach = () =>
    run(async () => {
      await call("POST", `/v1/teams/${team}/projects`, { repo: project.id, name: project.name });
      await backend.cloud.link(target, { mode: "sync", auto: false, confirmed: true });
      await loadTeam();
      setNotice(t("team.attached"));
    });

  if (targets.length === 0) {
    return (
      <div className="panel">
        <h2>{t("team.title")}</h2>
        <p className="muted">{t("team.noCloud")}</p>
      </div>
    );
  }

  return (
    <div className="sim">
      <aside className="sim-form" aria-label={t("team.title")}>
        <label className="field">
          <span>{t("team.server")}</span>
          <select value={target} onChange={(event) => setTarget(event.target.value)}>
            {targets.map((item) => (
              <option key={item.id} value={item.id}>
                {item.name}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>{t("team.team")}</span>
          <select value={team} onChange={(event) => setTeam(event.target.value)}>
            {teams.map((item) => (
              <option key={item.id} value={item.id}>
                {item.name}
              </option>
            ))}
          </select>
        </label>
        {myRole && <p className="muted hint">{t("team.yourRole", { role: t(`team.roles.${myRole}`) })}</p>}
        <form onSubmit={createTeam}>
          <label className="field">
            <span>{t("team.newTeam")}</span>
            <input value={form.teamName} onChange={(event) => set("teamName")(event.target.value)} />
          </label>
          <button type="submit" disabled={form.teamName.trim() === ""}>
            {t("team.create")}
          </button>
        </form>
        <form onSubmit={join}>
          <label className="field">
            <span>{t("team.joinCode")}</span>
            <input value={form.joinCode} spellCheck={false} onChange={(event) => set("joinCode")(event.target.value)} />
          </label>
          <button type="submit" disabled={form.joinCode.trim() === ""}>
            {t("team.join")}
          </button>
        </form>
        <h3>{t("team.notifications")}</h3>
        {notes.length === 0 && <p className="muted hint">{t("team.noNotifications")}</p>}
        <ul className="ai-findings">
          {notes.slice(0, 20).map((note) => (
            <li key={note.id} className={note.read ? "ai-finding" : "ai-finding ai-finding-warning"}>
              {t(`team.notes.${note.kind}`, { ...note.params, interpolation: { escapeValue: false } })}
            </li>
          ))}
        </ul>
        {notes.some((note) => !note.read) && (
          <button
            type="button"
            onClick={() =>
              void run(async () => {
                await call("POST", "/v1/notifications/read", {});
                await loadTeams();
              })
            }
          >
            {t("team.markRead")}
          </button>
        )}
      </aside>

      <div className="sim-results">
        {failure && (
          <p className="sim-warning" role="alert">
            {failure}
          </p>
        )}
        {notice && <p role="status">{notice}</p>}

        {team && (
          <section aria-label={t("team.members")}>
            <h2>{t("team.members")}</h2>
            <table className="sim-table">
              <tbody>
                {members.map((member) => (
                  <tr key={member.id}>
                    <td>{member.email}</td>
                    <td>{member.online ? t("team.online") : ""}</td>
                    <td>
                      {canManage ? (
                        <select
                          aria-label={t("team.roleOf", { email: member.email, interpolation: { escapeValue: false } })}
                          value={member.role}
                          onChange={(event) =>
                            void run(async () => {
                              await call("PATCH", `/v1/teams/${team}/members/${member.id}`, { role: event.target.value });
                              await loadTeam();
                            })
                          }
                        >
                          {ROLES.map((role) => (
                            <option key={role} value={role}>
                              {t(`team.roles.${role}`)}
                            </option>
                          ))}
                        </select>
                      ) : (
                        t(`team.roles.${member.role}`)
                      )}
                    </td>
                    <td>
                      {canManage && (
                        <button
                          type="button"
                          onClick={() =>
                            void run(async () => {
                              await call("DELETE", `/v1/teams/${team}/members/${member.id}`);
                              await loadTeam();
                            })
                          }
                        >
                          {t("team.removeMember")}
                        </button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {canManage && (
              <form onSubmit={invite} className="team-row">
                <label className="field">
                  <span>{t("team.inviteEmail")}</span>
                  <input value={form.email} onChange={(event) => set("email")(event.target.value)} />
                </label>
                <label className="field">
                  <span>{t("team.inviteRole")}</span>
                  <select value={form.role} onChange={(event) => set("role")(event.target.value)}>
                    {ROLES.map((role) => (
                      <option key={role} value={role}>
                        {t(`team.roles.${role}`)}
                      </option>
                    ))}
                  </select>
                </label>
                <button type="submit" disabled={form.email.trim() === ""}>
                  {t("team.invite")}
                </button>
              </form>
            )}
            {code && (
              <div className="review-note">
                <p>{t("team.codeHint")}</p>
                <code data-testid="invite-code" className="team-code">
                  {code}
                </code>
              </div>
            )}
          </section>
        )}

        {team && (
          <section aria-label={t("team.projects")}>
            <h2>{t("team.projects")}</h2>
            <table className="sim-table">
              <tbody>
                {projects.map((item) => (
                  <tr key={item.id}>
                    <td>{item.name}</td>
                    <td>{item.id === project.id ? t("team.thisProject") : ""}</td>
                    <td>
                      {item.id !== project.id && (
                        <button type="button" onClick={() => clone(item)}>
                          {t("team.clone")}
                        </button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {!inThisTeam && canManage && (
              <>
                <button type="button" onClick={() => void attach()}>
                  {t("team.attach")}
                </button>
                <p className="review-note">{t("team.attachNote")}</p>
              </>
            )}
          </section>
        )}

        {inThisTeam && access && (
          <section aria-label={t("team.areas")}>
            <h2>{t("team.areas")}</h2>
            <p className="muted hint">{t("team.areasHint")}</p>
            <p>{t("team.yourProjectRole", { role: t(`team.roles.${access.role}`) })}</p>
            <table className="sim-table">
              <tbody>
                {AREAS.map((area) => {
                  const lock = locks.find((item) => item.area === area);
                  const allowed = may("write_any_area") || (may("write_assigned_area") && access.areas.includes(area));
                  return (
                    <tr key={area}>
                      <td>{t(`team.areaNames.${area}`)}</td>
                      <td>
                        {lock
                          ? t("team.lockedBy", {
                              email: lock.mine ? t("team.you") : lock.email,
                              until: formatDate(lock.expires, i18n.language),
                              interpolation: { escapeValue: false },
                            })
                          : t("team.free")}
                      </td>
                      <td>
                        {!lock && allowed && (
                          <button
                            type="button"
                            onClick={() =>
                              void run(async () => {
                                await call("POST", `/v1/projects/${project.id}/locks`, { area });
                                await loadProject();
                              })
                            }
                          >
                            {t("team.lock", { area: t(`team.areaNames.${area}`) })}
                          </button>
                        )}
                        {lock && (lock.mine || may("manage_project")) && (
                          <button
                            type="button"
                            onClick={() =>
                              void run(async () => {
                                await call("DELETE", `/v1/projects/${project.id}/locks/${area}`);
                                await loadProject();
                              })
                            }
                          >
                            {lock.mine ? t("team.release") : t("team.override")}
                          </button>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
            {may("manage_project") && members.some((member) => member.role === "contributor") && (
              <>
                <h3>{t("team.assign")}</h3>
                <p className="muted hint">{t("team.assignHint")}</p>
                <table className="sim-table">
                  <tbody>
                    {members
                      .filter((member) => member.role === "contributor")
                      .map((member) => (
                        <tr key={member.id}>
                          <td>{member.email}</td>
                          {AREAS.map((area) => (
                            <td key={area}>
                              <label className="sim-traces">
                                <input
                                  type="checkbox"
                                  checked={grants.some((grant) => grant.account === member.id && grant.area === area)}
                                  onChange={(event) => {
                                    // Shown at once: the click must not seem to have no effect.
                                    const grant = event.target.checked;
                                    setGrants((current) =>
                                      grant
                                        ? [...current, { account: member.id, area }]
                                        : current.filter((item) => item.account !== member.id || item.area !== area),
                                    );
                                    void run(async () => {
                                      await call(grant ? "POST" : "DELETE", `/v1/projects/${project.id}/grants`, { account: member.id, area });
                                      await loadProject();
                                    });
                                  }}
                                />
                                {t(`team.areaNames.${area}`)}
                              </label>
                            </td>
                          ))}
                        </tr>
                      ))}
                  </tbody>
                </table>
              </>
            )}
            <label className="field">
              <span>{t("team.working")}</span>
              <select value={form.working} onChange={(event) => set("working")(event.target.value)}>
                {AREAS.map((area) => (
                  <option key={area} value={area}>
                    {t(`team.areaNames.${area}`)}
                  </option>
                ))}
              </select>
            </label>
            {here.length > 0 && (
              <p className="muted hint">
                {t("team.here", {
                  list: here.map((item) => `${item.email}: ${t(`team.areaNames.${item.area}`)}`).join(", "),
                  interpolation: { escapeValue: false },
                })}
              </p>
            )}
          </section>
        )}

        {inThisTeam && access && (
          <section aria-label={t("team.comments")}>
            <h2>{t("team.comments")}</h2>
            <ul className="ai-findings">
              {comments.map((comment) => (
                <li key={comment.id} className="ai-finding">
                  <small>
                    {comment.author}, {t(`team.areaNames.${comment.area}`)}
                    {comment.anchor && `, ${comment.anchor}`}, {formatDate(comment.created, i18n.language)}
                    {comment.resolved && `, ${t("team.resolved")}`}
                  </small>
                  <div className="ai-text">{comment.body}</div>
                  {!comment.resolved && may("comment") && (
                    <button
                      type="button"
                      onClick={() =>
                        void run(async () => {
                          await call("POST", `/v1/projects/${project.id}/comments/${comment.id}/resolve`, {});
                          await loadProject();
                        })
                      }
                    >
                      {t("team.resolve")}
                    </button>
                  )}
                </li>
              ))}
            </ul>
            {may("comment") && (
              <form onSubmit={addComment}>
                <div className="team-row">
                  <label className="field">
                    <span>{t("team.commentArea")}</span>
                    <select value={form.area} onChange={(event) => set("area")(event.target.value)}>
                      {AREAS.map((area) => (
                        <option key={area} value={area}>
                          {t(`team.areaNames.${area}`)}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="field">
                    <span>{t("team.commentAnchor")}</span>
                    <input value={form.anchor} onChange={(event) => set("anchor")(event.target.value)} />
                  </label>
                </div>
                <label className="field">
                  <span>{t("team.commentBody")}</span>
                  <textarea rows={2} value={form.comment} onChange={(event) => set("comment")(event.target.value)} />
                </label>
                <button type="submit" disabled={form.comment.trim() === ""}>
                  {t("team.commentSend")}
                </button>
              </form>
            )}
          </section>
        )}

        {team && canAudit && (
          <section aria-label={t("team.audit")}>
            <div className="ai-heading">
              <h2>{t("team.audit")}</h2>
              <button
                type="button"
                onClick={() =>
                  void run(async () => {
                    setAudit(await call("GET", `/v1/teams/${team}/audit`));
                  })
                }
              >
                {t("team.showAudit")}
              </button>
            </div>
            {audit && (
              <>
                <p className={audit.first_damaged === null ? "muted hint" : "sim-warning"} role="status">
                  {audit.first_damaged === null
                    ? t("team.chainIntact")
                    : t("team.chainDamaged", { seq: audit.first_damaged })}
                </p>
                <table className="sim-table">
                  <tbody>
                    {audit.entries.map((entry) => (
                      <tr key={entry.seq}>
                        <td>{formatDate(entry.at, i18n.language)}</td>
                        <td>{entry.actor_email}</td>
                        <td>{t(`team.auditActions.${entry.action.replace(".", "_")}`)}</td>
                        <td>{entry.detail}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </>
            )}
          </section>
        )}
      </div>
    </div>
  );
}
