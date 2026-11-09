import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ProjectView } from "./api/backend";
import { backend } from "./api/backend";
import App from "./App";
import i18n from "./i18n";

vi.mock("./api/backend", async (original) => {
  const actual = await original<typeof import("./api/backend")>();
  const stubbed = Object.fromEntries(Object.keys(actual.backend).map((key) => [key, vi.fn()]));
  return { ...actual, backend: stubbed };
});

const api = vi.mocked(backend);

function view(overrides: Partial<ProjectView> = {}): ProjectView {
  return {
    id: "id-1",
    path: "C:\\p\\Netzteil.tronic",
    name: "Netzteil",
    description: "",
    createdAt: "2026-11-04T10:00:00Z",
    modifiedAt: "2026-11-04T10:00:00Z",
    dirty: false,
    formatVersion: 1,
    recovery: null,
    ...overrides,
  };
}

beforeEach(async () => {
  vi.resetAllMocks();
  api.appInfo.mockResolvedValue({ version: "0.1.0", settingsPersistent: true, autosaveAvailable: true });
  api.getLanguage.mockResolvedValue(null);
  api.setLanguage.mockResolvedValue();
  api.recentProjects.mockResolvedValue([]);
  api.setWindowTitle.mockResolvedValue();
  api.onCloseRequested.mockResolvedValue(() => undefined);
  api.onAutosave.mockResolvedValue(() => undefined);
  api.recoveryList.mockResolvedValue([]);
  Object.assign(api, { cloud: { sync: vi.fn().mockResolvedValue({ results: [], project: null, links: [] }) } });
  Object.assign(api, {
    update: {
      status: vi.fn().mockRejectedValue("no backend"),
      confirm: vi.fn().mockResolvedValue(undefined),
      onChanged: vi.fn().mockResolvedValue(() => undefined),
    },
  });
  await i18n.changeLanguage("en-US");
});

async function openExisting(user: ReturnType<typeof userEvent.setup>, project = view()) {
  api.pickOpenPath.mockResolvedValue(project.path);
  api.openProject.mockResolvedValue(project);
  await user.click(screen.getByRole("button", { name: /Open project/ }));
  await screen.findByLabelText("Project name");
}

describe("project workflow", () => {
  it("creates a project through the dialog", async () => {
    const user = userEvent.setup();
    api.pickSavePath.mockResolvedValue("C:\\p\\Neu.tronic");
    api.createProject.mockResolvedValue(view({ name: "Neu", path: "C:\\p\\Neu.tronic" }));
    render(<App />);

    await user.click(screen.getByRole("button", { name: /New project/ }));
    const dialog = screen.getByRole("dialog");
    expect(dialog).toBeInTheDocument();
    await user.type(screen.getByLabelText("Project name"), "Neu");
    await user.click(screen.getByRole("button", { name: "Choose…" }));
    await screen.findByText("C:\\p\\Neu.tronic");
    await user.click(screen.getByRole("button", { name: "Create" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(api.createProject).toHaveBeenCalledWith("C:\\p\\Neu.tronic", "Neu", "");
    expect(screen.getByLabelText("Project name")).toHaveValue("Neu");
    expect(screen.getByRole("status")).toHaveTextContent("Project created");
  });

  it("does not create without a location", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(screen.getByRole("button", { name: /New project/ }));
    await user.click(screen.getByRole("button", { name: "Create" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Please choose a location.");
    expect(api.createProject).not.toHaveBeenCalled();
  });

  it("shows translated backend errors in the new project dialog and keeps the input", async () => {
    const user = userEvent.setup();
    api.pickSavePath.mockResolvedValue("C:\\p\\Neu.tronic");
    api.createProject.mockRejectedValue({
      code: "project.alreadyExists",
      params: { path: "C:\\p\\Neu.tronic" },
    });
    render(<App />);
    await user.click(screen.getByRole("button", { name: /New project/ }));
    await user.type(screen.getByLabelText("Project name"), "Neu");
    await user.click(screen.getByRole("button", { name: "Choose…" }));
    await screen.findByText("C:\\p\\Neu.tronic");
    await user.click(screen.getByRole("button", { name: "Create" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("already exists");
    expect(screen.getByLabelText("Project name")).toHaveValue("Neu");
  });

  it("opens, edits and saves a project", async () => {
    const user = userEvent.setup();
    render(<App />);
    await openExisting(user);

    api.updateProject.mockResolvedValue(view({ name: "Netzteil2", dirty: true }));
    await user.type(screen.getByLabelText("Project name"), "2");
    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent("Unsaved changes"),
    );
    expect(api.updateProject).toHaveBeenLastCalledWith("Netzteil2", "");

    api.saveProject.mockResolvedValue(view({ name: "Netzteil2" }));
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Saved"));
    expect(api.saveProject).toHaveBeenCalledWith(false);
  });

  it("shows open failures without leaving the start screen", async () => {
    const user = userEvent.setup();
    api.pickOpenPath.mockResolvedValue("C:\\p\\kaputt.tronic");
    api.openProject.mockRejectedValue({
      code: "project.checksumMismatch",
      params: { path: "C:\\p\\kaputt.tronic" },
    });
    render(<App />);
    await user.click(screen.getByRole("button", { name: /Open project/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent("failed its integrity check");
    expect(screen.queryByLabelText("Project name")).not.toBeInTheDocument();
  });

  it("disables saving while the metadata is invalid", async () => {
    const user = userEvent.setup();
    render(<App />);
    await openExisting(user);

    api.updateProject.mockRejectedValue({ code: "validation.nameEmpty", params: {} });
    await user.clear(screen.getByLabelText("Project name"));
    expect(await screen.findByRole("alert")).toHaveTextContent("Please enter a project name.");
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });
});

describe("protection of unsaved work", () => {
  async function dirtyProject(user: ReturnType<typeof userEvent.setup>) {
    render(<App />);
    await openExisting(user);
    api.updateProject.mockResolvedValue(view({ name: "NetzteilX", dirty: true }));
    await user.type(screen.getByLabelText("Project name"), "X");
    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent("Unsaved changes"),
    );
  }

  it("asks before closing a modified project, and cancel keeps it open", async () => {
    const user = userEvent.setup();
    await dirtyProject(user);
    await user.click(screen.getByRole("button", { name: "Close" }));
    expect(screen.getByRole("dialog")).toHaveTextContent("unsaved changes");
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(api.closeProject).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Project name")).toBeInTheDocument();
  });

  it("discard closes without saving", async () => {
    const user = userEvent.setup();
    await dirtyProject(user);
    api.closeProject.mockResolvedValue();
    await user.click(screen.getByRole("button", { name: "Close" }));
    await user.click(screen.getByRole("button", { name: "Don't save" }));
    await waitFor(() => expect(api.closeProject).toHaveBeenCalled());
    expect(api.saveProject).not.toHaveBeenCalled();
  });

  it("does not close when saving from the prompt fails", async () => {
    const user = userEvent.setup();
    await dirtyProject(user);
    api.saveProject.mockRejectedValue({ code: "project.io", params: { path: "p", detail: "x" } });
    await user.click(screen.getByRole("button", { name: "Close" }));
    const dialog = screen.getByRole("dialog");
    await user.click(dialog.querySelector("button.primary") as HTMLElement);
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not access");
    expect(api.closeProject).not.toHaveBeenCalled();
  });

  it("blocks closing the window while there are unsaved changes", async () => {
    const user = userEvent.setup();
    await dirtyProject(user);
    const shouldClose = api.onCloseRequested.mock.lastCall?.[0];
    let allowed: boolean | undefined;
    const { act } = await import("@testing-library/react");
    act(() => {
      allowed = shouldClose?.();
    });
    expect(allowed).toBe(false);
    api.destroyWindow.mockResolvedValue();
    api.closeProject.mockResolvedValue();
    await user.click(await screen.findByRole("button", { name: "Don't save" }));
    await waitFor(() => expect(api.destroyWindow).toHaveBeenCalled());
    expect(api.closeProject).toHaveBeenCalled();
  });

  it("offers overwrite or save as on an external change and overwrites only on request", async () => {
    const user = userEvent.setup();
    await dirtyProject(user);
    api.saveProject.mockRejectedValueOnce({
      code: "project.conflict",
      params: { path: "C:\\p\\Netzteil.tronic" },
    });
    await user.click(screen.getByRole("button", { name: "Save" }));
    await screen.findByRole("dialog");
    expect(api.saveProject).toHaveBeenCalledTimes(1);

    api.saveProject.mockResolvedValueOnce(view({ name: "NetzteilX" }));
    await user.click(screen.getByRole("button", { name: "Overwrite" }));
    await waitFor(() => expect(api.saveProject).toHaveBeenLastCalledWith(true));
  });
});

describe("recovery after a crash", () => {
  const entry = {
    path: "C:\\p\\Netzteil.tronic",
    name: "Netzteil",
    savedAt: "2026-11-04T10:00:00Z",
    diskState: "unchanged" as const,
  };

  it("offers left-behind work on start and restores it as unsaved", async () => {
    const user = userEvent.setup();
    api.recoveryList.mockResolvedValue([entry]);
    api.recoverProject.mockResolvedValue(view({ dirty: true }));
    render(<App />);
    expect(await screen.findByRole("dialog")).toHaveTextContent("Netzteil");
    await user.click(screen.getByRole("button", { name: "Restore" }));
    expect(api.recoverProject).toHaveBeenCalledWith(entry.path);
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Unsaved changes"));
    expect(screen.getByLabelText("Project name")).toHaveValue("Netzteil");
  });

  it("warns when the file changed in the meantime", async () => {
    api.recoveryList.mockResolvedValue([{ ...entry, diskState: "changed" }]);
    render(<App />);
    expect(await screen.findByRole("dialog")).toHaveTextContent("changed since");
  });

  it("discards only on request, one entry at a time", async () => {
    const user = userEvent.setup();
    const second = { ...entry, path: "C:\\p\\b.tronic", name: "Zweites" };
    api.recoveryList.mockResolvedValue([entry, second]);
    api.discardRecovery.mockResolvedValue();
    render(<App />);
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("Zweites");
    await user.click(screen.getAllByRole("button", { name: "Discard" })[0] as HTMLElement);
    await waitFor(() => expect(screen.getByRole("dialog")).not.toHaveTextContent("Netzteil"));
    expect(api.discardRecovery).toHaveBeenCalledWith(entry.path);
    expect(screen.getByRole("dialog")).toHaveTextContent("Zweites");
    await user.click(screen.getByRole("button", { name: "Discard" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("keeps everything when postponed", async () => {
    const user = userEvent.setup();
    api.recoveryList.mockResolvedValue([entry]);
    render(<App />);
    await screen.findByRole("dialog");
    await user.click(screen.getByRole("button", { name: "Decide later" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(api.discardRecovery).not.toHaveBeenCalled();
    expect(api.recoverProject).not.toHaveBeenCalled();
  });

  it("asks when an opened file has left-behind work", async () => {
    const user = userEvent.setup();
    render(<App />);
    api.pickOpenPath.mockResolvedValue(entry.path);
    api.openProject.mockResolvedValue(view({ recovery: entry }));
    await user.click(screen.getByRole("button", { name: /Open project/ }));
    expect(await screen.findByRole("dialog")).toHaveTextContent("Netzteil");
  });

  it("shows the time and failures of automatic saves", async () => {
    const user = userEvent.setup();
    render(<App />);
    await openExisting(user);
    await waitFor(() => expect(api.onAutosave).toHaveBeenCalled());
    const handler = api.onAutosave.mock.lastCall?.[0];
    const { act } = await import("@testing-library/react");
    act(() => handler?.({ savedAt: "2026-11-04T10:00:00Z", error: null }));
    expect(await screen.findByText(/Saved automatically/)).toBeInTheDocument();
    act(() => handler?.({ savedAt: null, error: { code: "project.io", params: { path: "p", detail: "voll" } } }));
    expect(await screen.findByText(/Automatic save failed/)).toHaveTextContent("voll");
  });
});

describe("language switching", () => {
  it.each([
    ["de-DE", "Willkommen bei TronicCAD"],
    ["zh-CN", "欢迎使用 TronicCAD"],
    ["fr-FR", "Bienvenue dans TronicCAD"],
    ["en-US", "Welcome to TronicCAD"],
  ])("switches the interface to %s and stores the choice", async (code, heading) => {
    const user = userEvent.setup();
    await i18n.changeLanguage(code === "en-US" ? "de-DE" : "en-US");
    render(<App />);
    await user.selectOptions(screen.getByRole("combobox"), code);
    expect(await screen.findByRole("heading", { name: heading })).toBeInTheDocument();
    expect(api.setLanguage).toHaveBeenCalledWith(code);
    expect(document.documentElement.lang).toBe(code);
  });

  it("restores the stored language on start", async () => {
    api.getLanguage.mockResolvedValue("fr-FR");
    render(<App />);
    expect(await screen.findByRole("heading", { name: "Bienvenue dans TronicCAD" })).toBeInTheDocument();
  });

  it("keeps unsaved edits when the language changes", async () => {
    const user = userEvent.setup();
    render(<App />);
    await openExisting(user);
    api.updateProject.mockResolvedValue(view({ name: "NetzteilX", dirty: true }));
    await user.type(screen.getByLabelText("Project name"), "X");
    await user.selectOptions(screen.getByRole("combobox"), "de-DE");
    expect(await screen.findByLabelText("Projektname")).toHaveValue("NetzteilX");
    expect(screen.getByRole("status")).toHaveTextContent("Ungespeicherte Änderungen");
  });
});

describe("honesty of the interface", () => {
  it("announces an available security update in the status bar", async () => {
    Object.assign(api, {
      update: {
        status: vi.fn().mockResolvedValue({ available: { version: "9.0.0", kind: "security" } }),
        confirm: vi.fn().mockResolvedValue(undefined),
        onChanged: vi.fn().mockResolvedValue(() => undefined),
      },
    });
    render(<App />);
    expect(await screen.findByRole("button", { name: /Security update 9\.0\.0/ })).toBeInTheDocument();
  });

  it("tells the waiting previous program that this one started", async () => {
    render(<App />);
    await waitFor(() => expect(api.update.confirm).toHaveBeenCalled());
  });

  it("names no module as planned any more", () => {
    render(<App />);
    expect(screen.queryByText(/Planned for phase/)).toBeNull();
  });

});

describe("switching between files", () => {
  it("shows the new content when a copy with the same project id is opened", async () => {
    const user = userEvent.setup();
    render(<App />);
    await openExisting(user, view({ name: "Original", path: "C:\p\a.tronic" }));
    expect(screen.getByLabelText("Project name")).toHaveValue("Original");

    api.pickOpenPath.mockResolvedValue("C:\p\b.tronic");
    api.openProject.mockResolvedValue(view({ name: "Kopie", path: "C:\p\b.tronic" }));
    await user.click(screen.getByRole("button", { name: "Open" }));
    await waitFor(() => expect(screen.getByLabelText("Project name")).toHaveValue("Kopie"));
  });
});
