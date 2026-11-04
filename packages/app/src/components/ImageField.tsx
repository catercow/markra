import { Button, IconButton, TextInput } from "@markra/ui";
import { ImagePlus, X } from "lucide-react";
import { useEffect, useRef, useState, type ClipboardEvent, type DragEvent } from "react";

/**
 * Marks an element that takes an image file dropped from the system. The desktop app's native
 * drag-drop sees the file before the page does, finds this element under the drop point and
 * hands it the file as an `imageFileDropEvent` (see App `handleDroppedLocalImage`).
 */
export const imageDropTargetAttribute = "data-image-drop-target";
export const imageFileDropEvent = "markra:image-file-drop";
export type ImageFileDropDetail = { file: File };

function imageFile(files: FileList | null | undefined) {
  return [...(files ?? [])].find((file) => file.type.startsWith("image/")) ?? null;
}

function hasFiles(event: DragEvent) {
  return [...(event.dataTransfer?.types ?? [])].includes("Files");
}

export type ImageFieldProps = {
  id: string;
  value: string;
  /** Typed text, or the path of an image just stored. */
  onChange: (src: string) => unknown;
  /** Shows a remove button while there is a value. */
  onClear?: () => unknown;
  /** A displayable URL for the value, for the thumbnail; no thumbnail without it. */
  resolveSrc?: (src: string) => string;
  /** Asks for an image and stores it; resolves its path. No Choose button without it. */
  pick?: () => Promise<string | null>;
  /** Stores a dropped or pasted image; resolves its path. Drops and pastes are ignored without it. */
  save?: (file: File) => Promise<string | null>;
  chooseLabel: string;
  removeLabel: string;
  "aria-describedby"?: string;
  "aria-label"?: string;
  placeholder?: string;
};

/** One image reference: a thumbnail, the path as text, Choose, remove, and drop or paste. */
export function ImageField({
  chooseLabel,
  id,
  onChange,
  onClear,
  pick,
  placeholder,
  removeLabel,
  resolveSrc,
  save,
  value,
  ...aria
}: ImageFieldProps) {
  const [draft, setDraft] = useState(value);
  const ownEdit = useRef(false);
  const [broken, setBroken] = useState(false);
  const [dragging, setDragging] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const latest = useRef({ onChange, save });
  latest.current = { onChange, save };

  useEffect(() => {
    if (!ownEdit.current) setDraft(value);
  }, [value]);
  useEffect(() => {
    ownEdit.current = false;
  });
  useEffect(() => setBroken(false), [value]);

  const store = async (file: File) => {
    const saved = await latest.current.save?.(file);
    if (saved) latest.current.onChange(saved);
  };

  useEffect(() => {
    const element = rootRef.current;
    if (!element || !save) return;
    const handle = (event: Event) => {
      const file = (event as CustomEvent<ImageFileDropDetail>).detail?.file;
      if (file) store(file).catch(() => {});
    };
    element.addEventListener(imageFileDropEvent, handle);
    return () => element.removeEventListener(imageFileDropEvent, handle);
  }, [Boolean(save)]);

  const accepts = Boolean(save);
  const dropProps = accepts
    ? {
        [imageDropTargetAttribute]: "",
        onDragLeave: () => setDragging(false),
        onDragOver: (event: DragEvent) => {
          if (!hasFiles(event)) return;
          event.preventDefault();
          setDragging(true);
        },
        onDrop: (event: DragEvent) => {
          const file = imageFile(event.dataTransfer?.files);
          setDragging(false);
          if (!file) return;
          event.preventDefault();
          store(file).catch(() => {});
        },
      }
    : {};

  return (
    <div
      className={`grid gap-1.5 rounded-md ${dragging ? "outline-2 outline-offset-2 outline-(--accent)" : ""}`}
      ref={rootRef}
      {...dropProps}
    >
      {value && resolveSrc && !broken ? (
        <img
          alt=""
          className="max-h-40 w-full rounded-md border border-(--border-default) bg-(--bg-secondary) object-contain"
          src={resolveSrc(value)}
          onError={() => setBroken(true)}
        />
      ) : null}
      <div className="flex items-center gap-1">
        <TextInput
          {...aria}
          className="min-w-0 flex-1"
          id={id}
          placeholder={placeholder}
          spellCheck={false}
          value={draft}
          onBlur={() => setDraft(value)}
          onChange={(event) => {
            ownEdit.current = true;
            setDraft(event.currentTarget.value);
            onChange(event.currentTarget.value);
          }}
          onPaste={(event: ClipboardEvent<HTMLInputElement>) => {
            const file = accepts ? imageFile(event.clipboardData?.files) : null;
            if (!file) return;
            event.preventDefault();
            store(file).catch(() => {});
          }}
        />
        {value && onClear ? (
          <IconButton label={removeLabel} size="icon-sm" onClick={onClear}>
            <X aria-hidden="true" size={13} />
          </IconButton>
        ) : null}
      </div>
      {pick ? (
        <Button
          className="justify-self-start"
          size="sm"
          onClick={async () => {
            const picked = await pick();
            if (picked) onChange(picked);
          }}
        >
          <ImagePlus aria-hidden="true" size={13} />
          {chooseLabel}
        </Button>
      ) : null}
    </div>
  );
}
