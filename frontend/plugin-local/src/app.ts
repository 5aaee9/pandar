type Config = {
  webUrl: string;
  hubUrl: string;
  callbackUrl: string;
  usingDefaultServer: boolean;
  usingDefaultWebServer: boolean;
  usingDefaultHubServer: boolean;
  configNonce: string;
};

type DiscoveryConfig = {
  hubUrl: string;
};

type StudioWindow = Window & {
  wx?: {
    postMessage?: (message: string) => void;
  };
};

type StudioLocalhostMessage = {
  command?: string;
  response?: {
    base_url?: string;
  };
  sequence_id?: string;
};

const form = document.querySelector<HTMLFormElement>("#target-form")!;
const webUrlInput = document.querySelector<HTMLInputElement>("#web-url")!;
const noticeList = document.querySelector<HTMLDivElement>("#notice-list")!;
const statusMessage = document.querySelector<HTMLDivElement>("#status")!;
const continueLink =
  document.querySelector<HTMLAnchorElement>("#continue-link")!;
const submitButton = form.querySelector<HTMLButtonElement>("button")!;

let callbackUrl = "";
let savedWebUrl = "";
let usingDefaultWebServer = false;
let configNonce = "";
let isDirty = true;

const setStatus = (message: string, isError = false) => {
  statusMessage.textContent = message;
  statusMessage.classList.toggle("error", isError);
};

const updateContinueLink = () => {
  continueLink.classList.toggle(
    "disabled",
    isDirty || !savedWebUrl || !callbackUrl,
  );
  continueLink.setAttribute(
    "aria-disabled",
    String(isDirty || !savedWebUrl || !callbackUrl),
  );
  if (isDirty || !savedWebUrl || !callbackUrl) {
    continueLink.href = "#";
    return;
  }

  continueLink.href = buildSignInUrl(callbackUrl);
};

const buildSignInUrl = (redirectUrl: string) => {
  const url = new URL("/plugin-sign-in", savedWebUrl);
  url.searchParams.set("redirect_url", redirectUrl);
  return url.toString();
};

const requestStudioCallbackUrl = () =>
  new Promise<string | null>((resolve) => {
    const studioWindow = window as StudioWindow;
    if (typeof studioWindow.wx?.postMessage !== "function") {
      resolve(null);
      return;
    }

    const sequenceId = `pandar-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    const timeout = window.setTimeout(() => {
      window.removeEventListener("message", handleMessage);
      resolve(null);
    }, 2000);

    function handleMessage(event: MessageEvent) {
      let data: StudioLocalhostMessage;
      try {
        data =
          typeof event.data === "string" ? JSON.parse(event.data) : event.data;
      } catch {
        return;
      }

      if (
        data?.command === "get_localhost_url" &&
        data.sequence_id === sequenceId &&
        data.response?.base_url
      ) {
        window.clearTimeout(timeout);
        window.removeEventListener("message", handleMessage);
        resolve(`${data.response.base_url}/callback`);
      }
    }

    window.addEventListener("message", handleMessage);
    studioWindow.wx.postMessage(
      JSON.stringify({
        command: "get_localhost_url",
        sequence_id: sequenceId,
      }),
    );
  });

const markDirty = () => {
  isDirty = true;
  renderNotices();
  updateContinueLink();
};

const renderNotices = () => {
  const notices = [];

  if (usingDefaultWebServer) {
    notices.push(`Web URL is using the default: ${webUrlInput.value}`);
  }

  noticeList.replaceChildren(
    ...notices.map((notice) => {
      const element = document.createElement("p");
      element.className = "notice";
      element.textContent = notice;
      return element;
    }),
  );
};

const applyConfig = (config: Config) => {
  webUrlInput.value = config.webUrl;
  callbackUrl = config.callbackUrl;
  savedWebUrl = config.webUrl;
  usingDefaultWebServer = config.usingDefaultWebServer;
  configNonce = config.configNonce;
  isDirty = false;
  renderNotices();
  updateContinueLink();
};

const discoverHubUrl = async (webUrl: string) => {
  const discoveryUrl = new URL("/.well-known/pandar", webUrl);
  const response = await fetch(discoveryUrl);
  if (!response.ok) {
    throw new Error(`Hub discovery failed with ${response.status}`);
  }

  const config = (await response.json()) as DiscoveryConfig;
  return config.hubUrl;
};

const saveTargetServer = async (
  pendingMessage: string,
  successMessage: string,
) => {
  submitButton.disabled = true;
  setStatus(pendingMessage);

  try {
    const hubUrl = await discoverHubUrl(webUrlInput.value);
    const response = await fetch("/config", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        webUrl: webUrlInput.value,
        hubUrl,
        configNonce,
      }),
    });

    if (!response.ok) {
      throw new Error(`POST /config failed with ${response.status}`);
    }

    applyConfig((await response.json()) as Config);
    setStatus(successMessage);
  } catch (error) {
    setStatus(
      error instanceof Error
        ? error.message
        : "Could not update target server.",
      true,
    );
  } finally {
    submitButton.disabled = false;
  }
};

const loadConfig = async () => {
  const response = await fetch("/config");
  if (!response.ok) {
    throw new Error(`GET /config failed with ${response.status}`);
  }

  applyConfig((await response.json()) as Config);
  markDirty();
  await saveTargetServer("Discovering Hub URL...", "Target server ready.");
  const studioCallbackUrl = await requestStudioCallbackUrl();
  if (studioCallbackUrl) {
    callbackUrl = studioCallbackUrl;
    updateContinueLink();
  }
};

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  await saveTargetServer("Discovering Hub URL...", "Target server updated.");
});

continueLink.addEventListener("click", async (event) => {
  if (isDirty || !savedWebUrl || !callbackUrl) {
    event.preventDefault();
    setStatus("Switch Target server before continuing.", true);
    return;
  }

  event.preventDefault();
  const studioCallbackUrl = await requestStudioCallbackUrl();
  window.location.href = buildSignInUrl(studioCallbackUrl ?? callbackUrl);
});

webUrlInput.addEventListener("input", markDirty);

loadConfig().catch((error) => {
  setStatus(
    error instanceof Error ? error.message : "Could not load target server.",
    true,
  );
  updateContinueLink();
});
