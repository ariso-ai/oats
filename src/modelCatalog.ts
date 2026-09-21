import { NOTES_MODEL_OPTIONS, notesModelKey, type NotesModelId } from './notesModels';

/** What a model is for. Drives the Type column, and which install/download
 *  state in Settings a row reads from. */
export type ModelType = 'Notes' | 'Speech';

/** Where a model runs: on this machine, or behind a provider's API. */
export type ModelRuntime = 'local' | 'remote';

export interface CatalogModel {
  /** Stable row identity, and what a Speech selection persists. */
  key: string;
  name: string;
  type: ModelType;
  runtime: ModelRuntime;
  /** Shown on hover over the type icon: what the model does, and its exact id. */
  details: string;
  /** Present only on Notes rows — what a Notes selection persists. */
  notesModel?: NotesModelId;
}

/** Speech models, in display order. One entry today, but the user still picks
 *  an active one — the same way they pick a notes model — so a second entry
 *  needs no new selection machinery. */
export const SPEECH_MODELS: CatalogModel[] = [
  {
    key: 'speech:parakeet-tdt-0.6b-v3',
    name: 'Parakeet TDT 0.6B v3',
    type: 'Speech',
    runtime: 'local',
    details:
      'Transcribes meeting audio to text. parakeet-tdt-0.6b-v3, running on this device.',
  },
];

export const DEFAULT_SPEECH_MODEL_KEY = SPEECH_MODELS[0].key;

/** Coerce a persisted speech selection into one this build actually ships. */
export function parseSpeechModelKey(raw: unknown): string {
  return typeof raw === 'string' && SPEECH_MODELS.some((m) => m.key === raw)
    ? raw
    : DEFAULT_SPEECH_MODEL_KEY;
}

/** Every model Settings lists, notes models first. */
export function modelCatalog(): CatalogModel[] {
  const notes: CatalogModel[] = NOTES_MODEL_OPTIONS.map((option) => ({
    key: notesModelKey(option.value),
    name: option.label,
    type: 'Notes',
    runtime: option.value.kind === 'local' ? 'local' : 'remote',
    details: `Writes meeting notes and titles. ${option.value.id}, ${
      option.value.kind === 'local' ? 'running on this device' : 'called over the network'
    }.`,
    notesModel: option.value,
  }));
  return [...notes, ...SPEECH_MODELS];
}

const MB = 1024 * 1024;

/** A model's footprint on disk. `null` means "not known" — which is every
 *  local model today, until the backend can report a directory's size. */
export function formatModelSize(bytes: number | null): string {
  if (bytes == null) return '—';
  const mb = bytes / MB;
  if (mb >= 1000) return `${(mb / 1024).toFixed(1)} GB`;
  return `${Math.round(mb)} MB`;
}
