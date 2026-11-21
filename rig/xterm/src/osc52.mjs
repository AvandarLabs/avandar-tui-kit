/**
 * Pure Base64 and OSC 52 clipboard payload decoder shared between Node.js
 * scenario runners and the browser xterm.js bridge page.
 */

const STRICT_BASE64_PATTERN =
  /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/;

function createInvalidOsc52Result(rawValue, selection = '') {
  return {
    raw: typeof rawValue === 'string' ? rawValue : String(rawValue ?? ''),
    selection,
    decodedText: null,
    isQuery: false,
    isValid: false,
  };
}

function createValidOsc52Result(rawPayload, selection, decodedText, isQuery = false) {
  return {
    raw: rawPayload,
    selection,
    decodedText,
    isQuery,
    isValid: true,
  };
}

/**
 * Decodes a standard Base64 string into a `Uint8Array` using `globalThis.atob`
 * so both browser `xterm.js` PTY writes and OSC 52 UTF-8 decoding share one path.
 */
export function makeUint8ArrayFromBase64(encodedBase64) {
  const binaryString = globalThis.atob(encodedBase64);
  return Uint8Array.from({ length: binaryString.length }, (_, byteIndex) =>
    binaryString.charCodeAt(byteIndex),
  );
}

function base64ToUtf8String(encodedBase64) {
  const byteBuffer = makeUint8ArrayFromBase64(encodedBase64);
  const utf8Decoder = new TextDecoder('utf-8', { fatal: true });
  return utf8Decoder.decode(byteBuffer);
}

/**
 * Decodes an OSC 52 body (`<selection>;<base64>` or `<selection>;?`) registered
 * on `xterm.js`, returning `{ raw, selection, decodedText, isQuery, isValid }`.
 */
export function decodeOsc52Payload(rawPayload) {
  if (typeof rawPayload !== 'string') {
    return createInvalidOsc52Result(rawPayload);
  }
  const firstSemicolonIndex = rawPayload.indexOf(';');
  if (firstSemicolonIndex === -1) {
    return createInvalidOsc52Result(rawPayload);
  }
  const selection = rawPayload.slice(0, firstSemicolonIndex);
  const bodyPart = rawPayload.slice(firstSemicolonIndex + 1);
  if (bodyPart === '?') {
    return createValidOsc52Result(rawPayload, selection, null, true);
  }
  const normalizedBase64 = bodyPart.replace(/[\r\n\s]+/g, '');
  if (normalizedBase64.length === 0) {
    return createValidOsc52Result(rawPayload, selection, '', false);
  }
  if (!STRICT_BASE64_PATTERN.test(normalizedBase64)) {
    return createInvalidOsc52Result(rawPayload, selection);
  }
  try {
    return createValidOsc52Result(rawPayload, selection, base64ToUtf8String(normalizedBase64));
  } catch {
    return createInvalidOsc52Result(rawPayload, selection);
  }
}
