"use strict";
const form = document.querySelector("#target-form");
const webUrlInput = document.querySelector("#web-url");
const hubUrlInput = document.querySelector("#hub-url");
const noticeList = document.querySelector("#notice-list");
const statusMessage = document.querySelector("#status");
const continueLink = document.querySelector("#continue-link");
const submitButton = form.querySelector("button");
let callbackUrl = "";
let savedWebUrl = "";
let usingDefaultWebServer = false;
let usingDefaultHubServer = false;
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
    const signInUrl = new URL("/plugin-sign-in", savedWebUrl);
    signInUrl.searchParams.set("redirect_url", callbackUrl);
    continueLink.href = signInUrl.toString();
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
    if (usingDefaultHubServer) {
        notices.push(`Hub URL is using the default: ${hubUrlInput.value}`);
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
    hubUrlInput.value = config.hubUrl;
    callbackUrl = config.callbackUrl;
    savedWebUrl = config.webUrl;
    usingDefaultWebServer = config.usingDefaultWebServer;
    usingDefaultHubServer = config.usingDefaultHubServer;
    configNonce = config.configNonce;
    isDirty = false;
    renderNotices();
    updateContinueLink();
};
const loadConfig = async () => {
    const response = await fetch("/config");
    if (!response.ok) {
        throw new Error(`GET /config failed with ${response.status}`);
    }
    applyConfig((await response.json()));
};
form.addEventListener("submit", async (event) => {
    event.preventDefault();
    submitButton.disabled = true;
    setStatus("Switching target server...");
    try {
        const response = await fetch("/config", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({
                webUrl: webUrlInput.value,
                hubUrl: hubUrlInput.value,
                configNonce,
            }),
        });
        if (!response.ok) {
            throw new Error(`POST /config failed with ${response.status}`);
        }
        applyConfig((await response.json()));
        setStatus("Target server updated.");
    }
    catch (error) {
        setStatus(error instanceof Error
            ? error.message
            : "Could not update target server.", true);
    }
    finally {
        submitButton.disabled = false;
    }
});
continueLink.addEventListener("click", (event) => {
    if (isDirty || !savedWebUrl || !callbackUrl) {
        event.preventDefault();
        setStatus("Switch Target server before continuing.", true);
    }
});
webUrlInput.addEventListener("input", markDirty);
hubUrlInput.addEventListener("input", markDirty);
loadConfig().catch((error) => {
    setStatus(error instanceof Error ? error.message : "Could not load target server.", true);
    updateContinueLink();
});
