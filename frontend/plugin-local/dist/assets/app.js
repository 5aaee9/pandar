const form = document.querySelector("#target-form");
const webUrlInput = document.querySelector("#web-url");
const noticeList = document.querySelector("#notice-list");
const statusMessage = document.querySelector("#status");
const continueLink = document.querySelector("#continue-link");
const submitButton = form.querySelector("button");
let callbackUrl = "";
let savedWebUrl = "";
let usingDefaultWebServer = false;
let configNonce = "";
let isDirty = true;
const setStatus = (message, isError = false) => {
    statusMessage.textContent = message;
    statusMessage.classList.toggle("error", isError);
};
const updateContinueLink = () => {
    continueLink.classList.toggle("disabled", isDirty || !savedWebUrl || !callbackUrl);
    continueLink.setAttribute("aria-disabled", String(isDirty || !savedWebUrl || !callbackUrl));
    if (isDirty || !savedWebUrl || !callbackUrl) {
        continueLink.href = "#";
        return;
    }
    continueLink.href = buildSignInUrl(callbackUrl);
};
const buildSignInUrl = (redirectUrl) => {
    const url = new URL("/plugin-sign-in", savedWebUrl);
    url.searchParams.set("redirect_url", redirectUrl);
    return url.toString();
};
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
    noticeList.replaceChildren(...notices.map((notice) => {
        const element = document.createElement("p");
        element.className = "notice";
        element.textContent = notice;
        return element;
    }));
};
const applyConfig = (config) => {
    webUrlInput.value = config.webUrl;
    callbackUrl = config.callbackUrl;
    savedWebUrl = config.webUrl;
    usingDefaultWebServer = config.usingDefaultWebServer;
    configNonce = config.configNonce;
    isDirty = false;
    renderNotices();
    updateContinueLink();
};
const discoverHubUrl = async (webUrl) => {
    const discoveryUrl = new URL("/.well-known/pandar", webUrl);
    const response = await fetch(discoveryUrl);
    if (!response.ok) {
        throw new Error(`Hub discovery failed with ${response.status}`);
    }
    const config = (await response.json());
    return config.hubUrl;
};
const saveTargetServer = async (pendingMessage, successMessage) => {
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
        applyConfig((await response.json()));
        setStatus(successMessage);
    }
    catch (error) {
        setStatus(error instanceof Error
            ? error.message
            : "Could not update target server.", true);
    }
    finally {
        submitButton.disabled = false;
    }
};
const loadConfig = async () => {
    const response = await fetch("/config");
    if (!response.ok) {
        throw new Error(`GET /config failed with ${response.status}`);
    }
    applyConfig((await response.json()));
    markDirty();
    await saveTargetServer("Discovering Hub URL...", "Target server ready.");
};
form.addEventListener("submit", async (event) => {
    event.preventDefault();
    await saveTargetServer("Discovering Hub URL...", "Target server updated.");
});
continueLink.addEventListener("click", (event) => {
    if (isDirty || !savedWebUrl || !callbackUrl) {
        event.preventDefault();
        setStatus("Switch Target server before continuing.", true);
        return;
    }
    event.preventDefault();
    window.location.href = buildSignInUrl(callbackUrl);
});
webUrlInput.addEventListener("input", markDirty);
loadConfig().catch((error) => {
    setStatus(error instanceof Error ? error.message : "Could not load target server.", true);
    updateContinueLink();
});
export {};
