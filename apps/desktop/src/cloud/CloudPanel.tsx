import { FormEvent, useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { backend, ProjectView } from "../api/backend";
import { Modal } from "../components/dialogs";
import { errorMessage, formatDate } from "../components/format";
import { Choice, Comparison, LinkView, Mode, Target, TargetKind, Version } from "./types";

type NewKind = TargetKind["kind"];
const KINDS: NewKind[] = ["folder", "web_dav", "s3", "cloud"];

const bytes = (value: number, language: string) =>
  value < 1024 * 1024
    ? new Intl.NumberFormat(language, { style: "unit", unit: "kilobyte", maximumFractionDigits: 1 }).format(value / 1024)
    : new Intl.NumberFormat(language, { style: "unit", unit: "megabyte", maximumFractionDigits: 2 }).format(value / 1024 / 1024);

function describe(target: Target): string {
  switch (target.kind) {
    case "folder":
      return target.path;
    case "web_dav":
      return target.url;
    case "s3":
      return `${target.endpoint} / ${target.bucket}`;
    case "cloud":
      return `${target.url} / ${target.email}`;
  }
}

const isExternal = (target: Target) => {
  if (target.kind === "folder") return false;
  const url = target.kind === "s3" ? target.endpoint : target.url;
  return !/^https?:\/\/(localhost|127\.\d+\.\d+\.\d+|\[::1\])(:|\/|$)/i.test(url);
};

function Field(props: { label: string; value: string; secret?: boolean; onChange: (value: string) => void }) {
  return (
    <label className="field">
      <span>{props.label}</span>
      <input
        type={props.secret ? "password" : "text"}
        value={props.value}
        autoComplete="off"
        spellCheck={false}
        onChange={(event) => props.onChange(event.target.value)}
      />
    </label>
  );
}

function AddTarget(props: { onAdded: (targets: Target[]) => void; onFailure: (message: string | null) => void }) {
  const { t } = useTranslation();
  const [kind, setKind] = useState<NewKind>("folder");
  const [form, setForm] = useState({ name: "", path: "", url: "", user: "", secret: "", bucket: "", region: "", accessKey: "", invitation: "" });
  const [create, setCreate] = useState(false);
  const [busy, setBusy] = useState(false);
  const set = (key: keyof typeof form) => (value: string) => setForm((current) => ({ ...current, [key]: value }));

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    props.onFailure(null);
    try {
      let targets: Target[];
      if (kind === "cloud") {
        targets = await backend.cloud.account(form.name, form.url, form.user, form.secret, create, form.invitation.trim() || null);
      } else {
        const target: TargetKind =
          kind === "folder"
            ? { kind, path: form.path }
            : kind === "web_dav"
              ? { kind, url: form.url, user: form.user }
              : { kind, endpoint: form.url, bucket: form.bucket, region: form.region, access_key: form.accessKey };
        targets = await backend.cloud.addTarget(form.name, target, form.secret);
      }
      props.onAdded(targets);
      setForm({ name: "", path: "", url: "", user: "", secret: "", bucket: "", region: "", accessKey: "", invitation: "" });
    } catch (error) {
      props.onFailure(errorMessage(t, error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form onSubmit={(event) => void submit(event)}>
      <h3>{t("cloud.add.title")}</h3>
      <label className="field">
        <span>{t("cloud.add.kind")}</span>
        <select value={kind} onChange={(event) => setKind(event.target.value as NewKind)}>
          {KINDS.map((item) => (
            <option key={item} value={item}>
              {t(`cloud.kinds.${item}`)}
            </option>
          ))}
        </select>
      </label>
      <Field label={t("cloud.add.name")} value={form.name} onChange={set("name")} />
      {kind === "folder" && (
        <>
          <Field label={t("cloud.add.path")} value={form.path} onChange={set("path")} />
          <button
            type="button"
            onClick={() => {
              void backend.pickDirectory().then((path) => {
                if (path) set("path")(path);
              });
            }}
          >
            {t("cloud.add.choose")}
          </button>
        </>
      )}
      {kind !== "folder" && <Field label={t("cloud.add.url")} value={form.url} onChange={set("url")} />}
      {kind === "web_dav" && (
        <>
          <Field label={t("cloud.add.user")} value={form.user} onChange={set("user")} />
          <Field label={t("cloud.add.password")} secret value={form.secret} onChange={set("secret")} />
        </>
      )}
      {kind === "s3" && (
        <>
          <Field label={t("cloud.add.bucket")} value={form.bucket} onChange={set("bucket")} />
          <Field label={t("cloud.add.region")} value={form.region} onChange={set("region")} />
          <Field label={t("cloud.add.accessKey")} value={form.accessKey} onChange={set("accessKey")} />
          <Field label={t("cloud.add.secretKey")} secret value={form.secret} onChange={set("secret")} />
        </>
      )}
      {kind === "cloud" && (
        <>
          <Field label={t("cloud.add.email")} value={form.user} onChange={set("user")} />
          <Field label={t("cloud.add.password")} secret value={form.secret} onChange={set("secret")} />
          <label className="sim-traces">
            <input type="checkbox" checked={create} onChange={(event) => setCreate(event.target.checked)} />
            {t("cloud.add.create")}
          </label>
          {create && <Field label={t("cloud.add.invitation")} value={form.invitation} onChange={set("invitation")} />}
          <p className="muted hint">{t("cloud.add.passwordRule")}</p>
        </>
      )}
      <button type="submit" className="primary" disabled={busy || form.name.trim() === ""}>
        {t("cloud.add.submit")}
      </button>
      <p className="muted hint">{t("cloud.add.secretsNote")}</p>
    </form>
  );
}

function ComparisonView({ comparison }: { comparison: Comparison }) {
  const { t } = useTranslation();
  if (comparison.identical) return <p>{t("cloud.compare.identical")}</p>;
  const list = (key: string, items: string[]) =>
    items.length > 0 && <li>{t(`cloud.compare.${key}`, { items: items.join(", "), interpolation: { escapeValue: false } })}</li>;
  return (
    <ul className="ai-findings" aria-label={t("cloud.compare.title")}>
      {list("partsAdded", comparison.parts_added)}
      {list("partsRemoved", comparison.parts_removed)}
      {comparison.values_changed.map((item) => (
        <li key={item.reference}>
          {t("cloud.compare.value", { ...item, interpolation: { escapeValue: false } })}
        </li>
      ))}
      {list("netsAdded", comparison.nets_added)}
      {list("netsRemoved", comparison.nets_removed)}
      {list("netsChanged", comparison.nets_changed)}
      {comparison.board_changed && (
        <li>
          {t("cloud.compare.board", {
            tracksBefore: comparison.tracks[0],
            tracksAfter: comparison.tracks[1],
            viasBefore: comparison.vias[0],
            viasAfter: comparison.vias[1],
            footprintsBefore: comparison.footprints[0],
            footprintsAfter: comparison.footprints[1],
          })}
        </li>
      )}
    </ul>
  );
}

export function CloudPanel(props: {
  project: ProjectView;
  onProject: (view: ProjectView) => void;
  onError: (error: unknown) => void;
}) {
  const { t, i18n } = useTranslation();
  const { project, onProject, onError } = props;
  const [targets, setTargets] = useState<Target[]>([]);
  const [device, setDevice] = useState("");
  const [links, setLinks] = useState<LinkView[]>([]);
  const [failure, setFailure] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [mode, setMode] = useState<Mode>("sync");
  const [confirmLink, setConfirmLink] = useState<Target | null>(null);
  const [conflict, setConflict] = useState<{ target: string; heads: string[] } | null>(null);
  const [versionsOf, setVersionsOf] = useState<string | null>(null);
  const [versions, setVersions] = useState<Version[]>([]);
  const [comparison, setComparison] = useState<{ id: string; result: Comparison } | null>(null);

  const refresh = useCallback(() => {
    backend.cloud
      .overview()
      .then((overview) => {
        setTargets(overview.targets);
        setDevice(overview.device);
      })
      .catch(onError);
    backend.cloud.links().then(setLinks).catch(onError);
  }, [onError]);

  useEffect(refresh, [refresh, project.id, project.path]);

  const nameOf = (id: string) => targets.find((target) => target.id === id)?.name ?? id;

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setFailure(null);
    setNotice(null);
    try {
      await action();
    } catch (error) {
      setFailure(errorMessage(t, error));
    } finally {
      setBusy(false);
    }
  };

  const link = (target: Target, confirmed: boolean) =>
    run(async () => {
      setLinks(await backend.cloud.link(target.id, { mode, auto: false, confirmed }));
    });

  const askLink = (target: Target) => {
    if (isExternal(target)) setConfirmLink(target);
    else void link(target, false);
  };

  const sync = () =>
    run(async () => {
      const report = await backend.cloud.sync(false);
      setLinks(report.links);
      if (report.project) onProject(report.project);
      const lines: string[] = [];
      for (const result of report.results) {
        const name = nameOf(result.target);
        if (result.error) {
          lines.push(`${name}: ${errorMessage(t, result.error)}`);
        } else if (result.merged) {
          const areas = (list: string[]) => list.map((area) => t(`team.areaNames.${area}`)).join(", ");
          lines.push(
            `${name}: ${t("cloud.merged", {
              here: areas(result.merged.from_here),
              there: areas(result.merged.from_there),
              interpolation: { escapeValue: false },
            })}`,
          );
          if (result.merged.deviations > 0) lines.push(t("cloud.mergedDeviations", { count: result.merged.deviations }));
        } else if (result.outcome?.kind === "conflict") {
          setConflict({ target: result.target, heads: result.outcome.heads });
          lines.push(`${name}: ${t("cloud.outcome.conflict")}`);
        } else if (result.outcome) {
          lines.push(`${name}: ${t(`cloud.outcome.${result.outcome.kind}`)}`);
        }
      }
      setNotice(lines.join(" · "));
    });

  const resolve = (choice: Choice) => {
    if (!conflict) return;
    const { target, heads } = conflict;
    setConflict(null);
    void run(async () => {
      const report = await backend.cloud.resolve(target, heads[0] as string, choice);
      setLinks(report.links);
      if (report.project) onProject(report.project);
      setNotice(report.copy ? t("cloud.conflict.copied", { path: report.copy, interpolation: { escapeValue: false } }) : t("cloud.conflict.done"));
    });
  };

  const showVersions = (target: string) =>
    run(async () => {
      setVersions(await backend.cloud.versions(target));
      setVersionsOf(target);
      setComparison(null);
    });

  return (
    <div className="sim">
      <aside className="sim-form" aria-label={t("cloud.targets")}>
        <h2>{t("cloud.targets")}</h2>
        {targets.length === 0 && <p className="muted hint">{t("cloud.noTargets")}</p>}
        <ul className="ai-findings">
          {targets.map((target) => (
            <li className="ai-finding" key={target.id}>
              <strong>{target.name}</strong>
              <div className="muted hint">{t(`cloud.kinds.${target.kind}`)}</div>
              <div className="muted hint cloud-address">{describe(target)}</div>
              <div className="sim-actions">
                {!links.some((item) => item.target === target.id) && (
                  <button type="button" disabled={busy} onClick={() => askLink(target)}>
                    {t("cloud.link")}
                  </button>
                )}
                <button
                  type="button"
                  disabled={busy}
                  onClick={() =>
                    void run(async () => {
                      setTargets(await backend.cloud.removeTarget(target.id));
                    })
                  }
                >
                  {t("cloud.remove")}
                </button>
              </div>
            </li>
          ))}
        </ul>
        <label className="field">
          <span>{t("cloud.newLinkMode")}</span>
          <select value={mode} onChange={(event) => setMode(event.target.value as Mode)}>
            <option value="sync">{t("cloud.modes.sync")}</option>
            <option value="backup">{t("cloud.modes.backup")}</option>
          </select>
        </label>
        <AddTarget onAdded={setTargets} onFailure={setFailure} />
        <label className="field">
          <span>{t("cloud.device")}</span>
          <input
            value={device}
            onChange={(event) => setDevice(event.target.value)}
            onBlur={() => void backend.cloud.setDevice(device).catch((error) => setFailure(errorMessage(t, error)))}
          />
        </label>
      </aside>

      <div className="sim-results">
        {failure && (
          <p className="sim-warning" role="alert">
            {failure}
          </p>
        )}
        {notice && <p role="status">{notice}</p>}
        <div className="ai-heading">
          <h2>{t("cloud.links")}</h2>
          <button type="button" className="primary" disabled={busy || links.length === 0} onClick={() => void sync()}>
            {busy ? t("cloud.working") : t("cloud.syncNow")}
          </button>
        </div>
        <p className="muted hint">{t("cloud.savedOnly")}</p>
        {links.length === 0 && <p className="muted">{t("cloud.noLinks")}</p>}
        <table className="sim-table">
          <tbody>
            {links.map((item) => (
              <tr key={item.target}>
                <td>{nameOf(item.target)}</td>
                <td>{t(`cloud.modes.${item.mode}`)}</td>
                <td>{t("cloud.linkStats", { versions: item.versions, size: bytes(item.stored_bytes, i18n.language) })}</td>
                <td>
                  <label className="sim-traces">
                    <input
                      type="checkbox"
                      checked={item.auto}
                      onChange={(event) => {
                        // Shown at once: the click must not seem to have no effect.
                        const auto = event.target.checked;
                        setLinks((current) => current.map((link) => (link.target === item.target ? { ...link, auto } : link)));
                        void run(async () => {
                          setLinks(await backend.cloud.link(item.target, { mode: item.mode, auto, confirmed: true }));
                        });
                      }}
                    />
                    {t("cloud.auto")}
                  </label>
                </td>
                <td>
                  <div className="sim-actions">
                    <button type="button" disabled={busy} onClick={() => void showVersions(item.target)}>
                      {t("cloud.versions")}
                    </button>
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() =>
                        void run(async () => {
                          setLinks(await backend.cloud.unlink(item.target));
                        })
                      }
                    >
                      {t("cloud.unlink")}
                    </button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <p className="review-note">{t("cloud.unlinkNote")}</p>

        {versionsOf && (
          <section aria-label={t("cloud.versions")}>
            <h3>{t("cloud.versionsOf", { name: nameOf(versionsOf), interpolation: { escapeValue: false } })}</h3>
            <table className="sim-table">
              <tbody>
                {versions.map((version) => (
                  <tr key={version.id}>
                    <td>{formatDate(version.commit.created, i18n.language)}</td>
                    <td>{version.commit.device}</td>
                    <td>{bytes(version.commit.size, i18n.language)}</td>
                    <td>{version.is_head ? t("cloud.newest") : ""}</td>
                    <td>
                      <div className="sim-actions">
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() =>
                            void run(async () => {
                              setComparison({ id: version.id, result: await backend.cloud.compare(versionsOf, version.id) });
                            })
                          }
                        >
                          {t("cloud.compareButton")}
                        </button>
                        <button
                          type="button"
                          disabled={busy || project.dirty}
                          onClick={() =>
                            void run(async () => {
                              onProject(await backend.cloud.restore(versionsOf, version.id));
                              setNotice(t("cloud.restored"));
                            })
                          }
                        >
                          {t("cloud.restore")}
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {comparison && (
              <>
                <h3>{t("cloud.compare.title")}</h3>
                <ComparisonView comparison={comparison.result} />
              </>
            )}
          </section>
        )}
      </div>

      {confirmLink && (
        <Modal title={t("cloud.consent.title")} onEscape={() => setConfirmLink(null)}>
          <p>{t("cloud.consent.body", { name: confirmLink.name, address: describe(confirmLink), interpolation: { escapeValue: false } })}</p>
          <p className="review-note">{t("cloud.consent.note")}</p>
          <div className="modal-actions">
            <button type="button" autoFocus onClick={() => setConfirmLink(null)}>
              {t("common.cancel")}
            </button>
            <button
              type="button"
              className="primary"
              onClick={() => {
                const target = confirmLink;
                setConfirmLink(null);
                void link(target, true);
              }}
            >
              {t("cloud.consent.agree")}
            </button>
          </div>
        </Modal>
      )}

      {conflict && (
        <Modal title={t("cloud.conflict.title")} onEscape={() => setConflict(null)}>
          <p>{t("cloud.conflict.body", { name: nameOf(conflict.target), interpolation: { escapeValue: false } })}</p>
          <p className="muted hint">{t("cloud.conflict.nothingLost")}</p>
          <div className="modal-actions cloud-choices">
            <button type="button" autoFocus onClick={() => setConflict(null)}>
              {t("cloud.conflict.later")}
            </button>
            <button type="button" onClick={() => resolve("take_remote")}>
              {t("cloud.conflict.takeRemote")}
            </button>
            <button type="button" onClick={() => resolve("keep_local")}>
              {t("cloud.conflict.keepLocal")}
            </button>
            <button type="button" className="primary" onClick={() => resolve("keep_both")}>
              {t("cloud.conflict.keepBoth")}
            </button>
          </div>
        </Modal>
      )}
    </div>
  );
}
