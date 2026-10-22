import { expect, test } from "@playwright/test";
import { administrator } from "./accounts";
import { submitLogin } from "./login";

test("component dialogs and selectors comply with the page security policy", async ({ page }) => {
  await page.addInitScript(() => {
    const violations: string[] = [];
    Object.assign(window, { garageCspViolations: violations });
    document.addEventListener("securitypolicyviolation", event => violations.push(event.effectiveDirective));
  });
  await page.goto("/");
  await page.getByLabel("Username").fill(administrator.username);
  await page.getByLabel("Password").fill(administrator.password);
  await submitLogin(page);
  const nav = page.getByRole("navigation", { name: "Sections" });
  await nav.getByRole("link", { name: "Keys", exact: true }).click();
  await page.getByRole("button", { name: "Create", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "Create key", exact: true })).toBeVisible();
  await page.getByRole("dialog", { name: "Create key", exact: true }).getByRole("button", { name: "Cancel", exact: true }).click();
  await nav.getByRole("link", { name: "Nodes and layout", exact: true }).click();
  await page.getByRole("link", { name: /^Open maintenance for / }).first().click();
  await page.getByRole("combobox", { name: "Unit", exact: true }).click();
  await page.getByRole("option", { name: "GB", exact: true }).click();
  const violations = await page.evaluate(() => (window as unknown as { garageCspViolations: string[] }).garageCspViolations);
  expect(violations).toEqual([]);
  const blocked = await page.evaluate(() => {
    const script = document.createElement("script");
    script.textContent = "window.garageInlineScriptRan = true";
    document.head.append(script);
    script.remove();
    const style = document.createElement("style");
    style.textContent = "html { --garage-inline-style-probe: applied; }";
    document.head.append(style);
    const inlineStyle = getComputedStyle(document.documentElement).getPropertyValue("--garage-inline-style-probe").trim();
    style.remove();
    return { script: Boolean((window as unknown as { garageInlineScriptRan?: boolean }).garageInlineScriptRan), style: inlineStyle };
  });
  expect(blocked).toEqual({ script: false, style: "" });
});
