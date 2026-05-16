async function fetchPartial(path) {
  const response = await fetch(new URL(path, window.location.href));

  if (!response.ok) {
    throw new Error(`Failed to load partial: ${path}`);
  }

  return response.text();
}

function replaceSlot(slot, html) {
  const template = document.createElement("template");
  template.innerHTML = html.trim();
  slot.replaceWith(template.content);
}

function applyPageVariants(page) {
  for (const element of document.querySelectorAll("[data-page-only]")) {
    if (element.dataset.pageOnly !== page) {
      element.remove();
    }
  }

  const randomizeButton = document.getElementById("btn-randomize");
  if (randomizeButton && page === "progression") {
    randomizeButton.textContent = "RANDOMIZE 1-4";
    randomizeButton.classList.add("td3-randomize-btn--secondary");
  }
}

function dispatchLayoutReady(page) {
  document.dispatchEvent(new CustomEvent("layout:ready", { detail: { page } }));
  window.dispatchEvent(new CustomEvent("layout:ready", { detail: { page } }));
}

export async function bootstrapLayout({ page = document.body.dataset.page, entryScript } = {}) {
  if (!page) {
    throw new Error("Missing data-page for layout bootstrap.");
  }

  const slots = Array.from(document.querySelectorAll("[data-partial-path]"));
  const partials = await Promise.all(
    slots.map((slot) => fetchPartial(slot.dataset.partialPath)),
  );

  for (const [index, slot] of slots.entries()) {
    replaceSlot(slot, partials[index]);
  }

  applyPageVariants(page);

  if (entryScript) {
    await import(new URL(entryScript, window.location.href).href);
  }

  dispatchLayoutReady(page);
}
