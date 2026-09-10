/**
 * Mirrors the Rust `madari_native::playback::languages` labels so the browser and
 * the TV show the same names for a language code.
 */
const NAMES: Record<string, string> = {
  en: 'English', eng: 'English', hi: 'Hindi', hin: 'Hindi', ta: 'Tamil', tam: 'Tamil',
  te: 'Telugu', tel: 'Telugu', ml: 'Malayalam', mal: 'Malayalam', kn: 'Kannada', kan: 'Kannada',
  mr: 'Marathi', mar: 'Marathi', bn: 'Bengali', ben: 'Bengali', gu: 'Gujarati', guj: 'Gujarati',
  pa: 'Punjabi', pan: 'Punjabi', ur: 'Urdu', urd: 'Urdu', as: 'Assamese', asm: 'Assamese',
  or: 'Odia', ori: 'Odia', ne: 'Nepali', nep: 'Nepali', si: 'Sinhala', sin: 'Sinhala',
  fr: 'French', fra: 'French', fre: 'French', de: 'German', deu: 'German', ger: 'German',
  es: 'Spanish', spa: 'Spanish', pt: 'Portuguese', por: 'Portuguese', it: 'Italian', ita: 'Italian',
  nl: 'Dutch', nld: 'Dutch', dut: 'Dutch', pl: 'Polish', pol: 'Polish', ru: 'Russian', rus: 'Russian',
  uk: 'Ukrainian', ukr: 'Ukrainian', tr: 'Turkish', tur: 'Turkish', sv: 'Swedish', swe: 'Swedish',
  da: 'Danish', dan: 'Danish', fi: 'Finnish', fin: 'Finnish', no: 'Norwegian', nor: 'Norwegian',
  nb: 'Norwegian Bokmål', nn: 'Norwegian Nynorsk', el: 'Greek', ell: 'Greek', gre: 'Greek',
  cs: 'Czech', ces: 'Czech', sk: 'Slovak', slk: 'Slovak', hu: 'Hungarian', hun: 'Hungarian',
  ro: 'Romanian', ron: 'Romanian', bg: 'Bulgarian', bul: 'Bulgarian', hr: 'Croatian', hrv: 'Croatian',
  sr: 'Serbian', srp: 'Serbian', sl: 'Slovenian', slv: 'Slovenian', bs: 'Bosnian', bos: 'Bosnian',
  sq: 'Albanian', mk: 'Macedonian', et: 'Estonian', lv: 'Latvian', lt: 'Lithuanian',
  is: 'Icelandic', ca: 'Catalan', eu: 'Basque', gl: 'Galician', ka: 'Georgian', hy: 'Armenian',
  az: 'Azerbaijani', kk: 'Kazakh', uz: 'Uzbek', sw: 'Swahili', af: 'Afrikaans', zu: 'Zulu',
  ja: 'Japanese', jpn: 'Japanese', ko: 'Korean', kor: 'Korean', zh: 'Chinese', zho: 'Chinese',
  chi: 'Chinese', ar: 'Arabic', ara: 'Arabic', he: 'Hebrew', heb: 'Hebrew', fa: 'Persian',
  fas: 'Persian', per: 'Persian', th: 'Thai', tha: 'Thai', vi: 'Vietnamese', vie: 'Vietnamese',
  id: 'Indonesian', ind: 'Indonesian', ms: 'Malay', msa: 'Malay', may: 'Malay',
  tl: 'Filipino', tgl: 'Filipino', fil: 'Filipino',
  und: 'Unspecified language', mul: 'Multiple languages', zxx: 'No spoken language',
};

const QUALIFIERS: Record<string, string> = {
  us: 'United States', gb: 'United Kingdom', br: 'Brazil', pt: 'Portugal', ca: 'Canada', mx: 'Mexico',
  hans: 'Simplified', hant: 'Traditional', latn: 'Latin', cyrl: 'Cyrillic',
};

export function languageName(code: string): string {
  const normalized = code.trim().replace(/_/g, '-').toLowerCase();
  if (!normalized) return '';
  const [language, ...qualifiers] = normalized.split('-');
  const base = NAMES[language] ?? code.trim();
  if (!qualifiers.length) return base;
  const suffix = qualifiers.map((tag) => QUALIFIERS[tag] ?? tag.toUpperCase()).join(', ');
  return `${base} (${suffix})`;
}

/** Curated list offered when adding a language; unknown tags still round-trip. */
export const COMMON_LANGUAGES = [
  'en', 'hi', 'ta', 'te', 'ml', 'kn', 'mr', 'bn', 'gu', 'pa', 'ur', 'as', 'or', 'ne', 'si',
  'fr', 'de', 'es', 'pt', 'it', 'nl', 'pl', 'ru', 'uk', 'tr', 'sv', 'da', 'fi', 'no', 'el',
  'ja', 'ko', 'zh', 'ar', 'he', 'fa', 'th', 'vi', 'id', 'ms', 'tl',
];

export const TRACK_PREFERENCES: Array<{key: 'subtitle_sdh' | 'subtitle_forced' | 'audio_description' | 'audio_commentary'; label: string}> = [
  {key: 'subtitle_sdh', label: 'SDH subtitles'},
  {key: 'subtitle_forced', label: 'Forced subtitles'},
  {key: 'audio_description', label: 'Audio description'},
  {key: 'audio_commentary', label: 'Commentary tracks'},
];
