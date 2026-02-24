export interface TranscriptionEntry {
  id: string;
  timestamp: number;
  original_text: string;
  refined_text: string | null;
  language: string;
  audio_duration_ms: number;
  provider: string;
}
