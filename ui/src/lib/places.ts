// ui/src/lib/places.ts
//
// The Location field's autocomplete. Recent event locations plus, when
// the Photon setting is on, remote hits — distilled on the Rust side so
// the webview never talks to the internet. See `search_places` in
// `src-tauri/src/geocode.rs`.

import { invoke } from '@tauri-apps/api/core';

export type PlaceHit = {
  label: string;
  lat: number | null;
  lon: number | null;
  source: 'history' | 'search';
};

export const searchPlaces = (query: string) =>
  invoke<PlaceHit[]>('search_places', { query });
