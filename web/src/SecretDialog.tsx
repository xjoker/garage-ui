import { Button, Dialog, TextField } from "@radix-ui/themes";
import { useLayoutEffect, useRef, useState } from "react";
import { copyText } from "./clipboard";
import { copy } from "./i18n";

function copySelectedSecret(field: HTMLInputElement | null): boolean {
  if (!field) return false;
  const type = field.type;
  try {
    // Browsers can report success without copying a masked password field.
    field.type = "text";
    field.focus();
    field.select();
    return document.execCommand("copy");
  } catch {
    return false;
  } finally {
    field.type = type;
  }
}

export function SecretDialog({ secret, title, label, description, onClose, returnFocus }: {
  secret: string;
  title: string;
  label: string;
  description: string;
  onClose: () => void;
  returnFocus: HTMLButtonElement | null;
}) {
  const copyRef = useRef<HTMLButtonElement>(null);
  const secretRef = useRef<HTMLInputElement>(null);
  const [visible, setVisible] = useState(false);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [selectForManualCopy, setSelectForManualCopy] = useState(false);
  useLayoutEffect(() => {
    if (selectForManualCopy) {
      secretRef.current?.focus();
      secretRef.current?.select();
      setSelectForManualCopy(false);
    }
  }, [selectForManualCopy]);
  return (
    <Dialog.Root open onOpenChange={(open) => { if (!open) onClose(); }}>
      <Dialog.Content maxWidth="480px" onCloseAutoFocus={(event) => { event.preventDefault(); if (returnFocus?.isConnected) returnFocus.focus(); }} onOpenAutoFocus={(event) => { event.preventDefault(); copyRef.current?.focus(); }}>
        <Dialog.Title>{title}</Dialog.Title>
        <Dialog.Description>{description}</Dialog.Description>
        <label className="ui-field">
          <span>{label}</span>
          <TextField.Root size="2" readOnly autoComplete="off" type={visible ? "text" : "password"} value={secret} ref={secretRef} />
        </label>
        <div className="ui-actions">
          <Button size="2" variant="soft" color="gray" type="button" onClick={() => setVisible(true)}>{copy("ui.show.secret")}</Button>
          <Button size="2" variant="soft" color="gray" type="button" ref={copyRef} onClick={() => {
            void copyText(secret, () => copySelectedSecret(secretRef.current)).then((result) => {
              if (result === "copied") { setFeedback(copy("ui.copied")); copyRef.current?.focus(); return; }
              setVisible(true);
              setSelectForManualCopy(true);
              setFeedback(copy("ui.select.the.secret.and.copy.it"));
            });
          }}>{copy("ui.copy.secret")}</Button>
          <Button size="2" variant="soft" color="gray" type="button" onClick={onClose}>{copy("ui.i.have.saved.it")}</Button>
        </div>
        {feedback ? <p role="status">{feedback}</p> : null}
      </Dialog.Content>
    </Dialog.Root>
  );
}
