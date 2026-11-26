/**
 * How the host's props decide the way the visual editor reads a file: paper
 * reading mode infers merged table cells (R-BLK-11).
 *
 * Clean implementation for Lattice; spec: docs/visual-editor-spec.md.
 */
import { isPaperLibraryPath } from "../../../papers/paper-link";
import type { VisualMarkdownEditorProps } from "../visual-editor-props";
import type { OpenOptions } from "./markdown-document";

export const openOptions = (props: Pick<VisualMarkdownEditorProps, "activePath" | "optimizeForReading">): OpenOptions => ({
  paperSpans: Boolean(props.optimizeForReading && isPaperLibraryPath(props.activePath)),
});
