/**
 * Pure OSC 52 clipboard payload decoder shared between Node.js and browser xterm.js.
 */

const STRICT_BASE64_PATTERN =
  /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/;

function buildInvalidOsc52Result(rawValue, selection = '') {
  return {
    raw: typeof rawValue === 'string' ? rawValue : String(rawValue ?? ''),
    selection,
    decodedText: null,
    isQuery: false,
    isValid: false,
  };
}

function buildValidOsc52Result(rawPayload, selection, decodedText, isQuery = false) {
  return {
    raw: rawPayload,
    selection,
    decodedText,
    isQuery,
    isValid: true,
  };
}

function base64ToUtf8String(encodedBase64) {
  const binaryString = globalThis.atob(encodedBase64);
  const byteBuffer = new Uint8Array(binaryString.length);
  for (let byteIndex = 0; byteIndex < binaryString.length; byteIndex += 1) {
    byteBuffer[byteIndex] = binaryString.charCodeAt(byteIndex);
  }
  const utf8Decoder = new TextDecoder('utf-8', { fatal: true });
  return utf8Decoder.decode(byteBuffer);
}

/**
 * Decodes the payload delivered to `terminal.parser.registerOscHandler(52, ...)`,
 * expecting `<selection>;<base64>` or `<selection>;?`.
 */
export function decodeOsc52Payload(rawPayload) {
  if (typeof rawPayload !== 'string') {
    return buildInvalidOsc52Result(rawPayload);
  }
  const firstSemicolonIndex = rawPayload.indexOf(';');
  if (firstSemicolonIndex === -1) {
    return buildInvalidOsc52Result(rawPayload);
  }
  const selection = rawPayload.slice(0, firstSemicolonIndex);
  const bodyPart = rawPayload.slice(firstSemicolonIndex + 1);
  if (bodyPart === '?') {
    return buildValidOsc52Result(rawPayload, selection, null, true);
  }
  const normalizedBase64 = bodyPart.replace(/[\r\n\s]+/g, '');
  if (normalizedBase64.length === 0) {
    return buildValidOsc52Result(rawPayload, selection, '', false);
  }
  if (!STRICT_BASE64_PATTERN.test(normalizedBase64)) {
    return buildInvalidOsc52Result(rawPayload, selection);
  }
  try {
    return buildValidOsc52Result(rawPayload, selection, base64ToUtf8String(normalizedBase64));
  } catch {
    return buildInvalidOsc52Result(rawPayload, selection);
  }
}
