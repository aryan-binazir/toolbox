const form = document.querySelector("#unlock-form");
const passcode = document.querySelector("#mobile-passcode");
const error = document.querySelector("#unlock-error");

passcode?.focus();

form?.addEventListener("submit", async (event) => {
  event.preventDefault();
  error.textContent = "";
  const submit = form.querySelector('button[type="submit"]');
  submit.disabled = true;

  try {
    const response = await fetch("/api/unlock", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ passcode: passcode.value }),
    });
    if (!response.ok) {
      if (response.status === 401) {
        error.textContent = "Incorrect passcode.";
      } else if (response.status === 429) {
        const body = await response.json().catch(() => null);
        error.textContent = body?.error || "Too many attempts. Try again shortly.";
      } else {
        error.textContent = "Unable to unlock AI Scheduler.";
      }
      passcode.select();
      return;
    }
    window.location.replace("/");
  } catch (_) {
    error.textContent = "Unable to reach AI Scheduler.";
  } finally {
    submit.disabled = false;
  }
});
