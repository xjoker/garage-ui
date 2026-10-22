import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { ResourceIdentity } from "./ui";

beforeEach(() => {
  Object.defineProperty(window, "isSecureContext", { configurable: true, value: true });
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
const id = "GK0123456789abcdefghijklmnop";

test("resource identity copies the complete ID and confirms only after success", async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
  render(<ResourceIdentity id={id} />);
  expect(screen.getByText(id).tagName).toBe("CODE");
  fireEvent.click(screen.getByRole("button", { name: "Copy ID" }));
  await waitFor(() => expect(screen.getByRole("status").textContent).toBe("ID copied"));
  expect(writeText).toHaveBeenCalledWith(id);
});

test("resource identity reports failed clipboard writes without a success claim", async () => {
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: vi.fn().mockRejectedValue(new Error("blocked")) } });
  render(<ResourceIdentity id={id} />);
  fireEvent.click(screen.getByRole("button", { name: "Copy ID" }));
  await waitFor(() => expect(screen.getByRole("alert").textContent).toBe("Could not copy ID. Select the full ID and copy it manually."));
  expect(screen.queryByText("ID copied")).toBeNull();
});


test("compact resource identity keeps copying inside the full ID disclosure", async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
  const { container } = render(<ResourceIdentity id={id} compact />);
  const details = container.querySelector("details");
  if (!details) throw new Error("Identity disclosure is missing");
  expect(details.open).toBe(false);
  expect(details.contains(screen.getByRole("button", { name: "Copy ID" }))).toBe(true);
  details.open = true;
  fireEvent.click(screen.getByRole("button", { name: "Copy ID" }));
  await waitFor(() => expect(screen.getByRole("status").textContent).toBe("ID copied"));
  expect(writeText).toHaveBeenCalledWith(id);
});

test("HTTP ID copying exposes the full selection only during the copy operation", async () => {
  Object.defineProperty(window, "isSecureContext", { configurable: true, value: false });
  const execCommand = vi.fn(() => {
    const code = document.querySelector(".ui-resource-identity code");
    return code?.closest("details")?.open === true && window.getSelection()?.toString() === id;
  });
  Object.defineProperty(document, "execCommand", { configurable: true, value: execCommand });
  const { container } = render(<ResourceIdentity id={id} />);
  fireEvent.click(screen.getByRole("button", { name: "Copy ID" }));
  await waitFor(() => expect(screen.getByRole("status").textContent).toBe("ID copied"));
  expect(container.querySelector("details")?.open).toBe(false);
  expect(execCommand).toHaveBeenCalledWith("copy");
});
