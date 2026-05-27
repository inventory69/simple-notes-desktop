export const NOTE_COLORS = [
  { name: 'Red', light: '#F28B82', dark: '#7A3028' },
  { name: 'Orange', light: '#FBBC04', dark: '#6B4E00' },
  { name: 'Yellow', light: '#FFF475', dark: '#5C5200' },
  { name: 'Green', light: '#CCFF90', dark: '#2D5A0F' },
  { name: 'Teal', light: '#A7FFEB', dark: '#145C45' },
  { name: 'Blue', light: '#CBF0F8', dark: '#104F5C' },
  { name: 'Cerulean', light: '#AECBFA', dark: '#0D3360' },
  { name: 'Purple', light: '#D7AEFB', dark: '#3D1060' },
  { name: 'Pink', light: '#FDCFE8', dark: '#6B1940' },
  { name: 'Brown', light: '#E6C9A8', dark: '#4A2D10' },
  { name: 'Gray', light: '#E8EAED', dark: '#2E3135' },
];

/** Gibt {light, dark} für einen Hex-String zurück, oder null bei keiner Übereinstimmung */
export function getColorPair(hex) {
  if (!hex) return null;
  return NOTE_COLORS.find((c) => c.light.toLowerCase() === hex.toLowerCase()) ?? null;
}
