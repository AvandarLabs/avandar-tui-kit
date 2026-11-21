/**
 * Local HTTP + WebSocket bridge driving browser `@xterm/xterm@6.0.0` via Playwright Chromium.
 */

import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { WebSocketServer } from 'ws';

const MODULE_DIRECTORY = path.dirname(fileURLToPath(import.meta.url));
const PACKAGE_ROOT = path.resolve(MODULE_DIRECTORY, '..');

const STATIC_ROUTE_TABLE = Object.freeze({
  '/': {
    filePath: path.join(PACKAGE_ROOT, 'web', 'index.html'),
    contentType: 'text/html; charset=utf-8',
  },
  '/vendor/xterm.css': {
    filePath: path.join(PACKAGE_ROOT, 'node_modules', '@xterm', 'xterm', 'css', 'xterm.css'),
    contentType: 'text/css; charset=utf-8',
  },
  '/vendor/xterm.js': {
    filePath: path.join(PACKAGE_ROOT, 'node_modules', '@xterm', 'xterm', 'lib', 'xterm.js'),
    contentType: 'text/javascript; charset=utf-8',
  },
  '/vendor/addon-unicode11.js': {
    filePath: path.join(
      PACKAGE_ROOT,
      'node_modules',
      '@xterm',
      'addon-unicode11',
      'lib',
      'addon-unicode11.js',
    ),
    contentType: 'text/javascript; charset=utf-8',
  },
  '/src/osc52.mjs': {
    filePath: path.join(PACKAGE_ROOT, 'src', 'osc52.mjs'),
    contentType: 'text/javascript; charset=utf-8',
  },
});

function serveStaticAsset(request, response) {
  const requestUrl = new URL(request.url ?? '/', 'http://127.0.0.1');
  if (requestUrl.pathname === '/favicon.ico') {
    response.writeHead(204);
    response.end();
    return;
  }
  const routeEntry = STATIC_ROUTE_TABLE[requestUrl.pathname];
  if (!routeEntry) {
    response.writeHead(404, { 'content-type': 'text/plain; charset=utf-8' });
    response.end('Not found');
    return;
  }
  const assetBytes = fs.readFileSync(routeEntry.filePath);
  response.writeHead(200, {
    'content-type': routeEntry.contentType,
    'cache-control': 'no-store',
  });
  response.end(assetBytes);
}

function listenOnEphemeralLoopbackPort(httpServer) {
  return new Promise((resolve, reject) => {
    httpServer.once('error', reject);
    httpServer.listen(0, '127.0.0.1', () => {
      const addressInfo = httpServer.address();
      resolve(typeof addressInfo === 'object' && addressInfo ? addressInfo.port : 0);
    });
  });
}

function closeHttpServer(httpServer) {
  return new Promise((resolve) => {
    if (!httpServer.listening) {
      resolve();
      return;
    }
    httpServer.closeAllConnections?.();
    httpServer.close(() => resolve());
  });
}

function chunkToBase64(chunk) {
  if (Buffer.isBuffer(chunk) || chunk instanceof Uint8Array) {
    return Buffer.from(chunk).toString('base64');
  }
  return Buffer.from(String(chunk), 'utf8').toString('base64');
}

function resolveSatisfiedAckWaiters(bridgeState) {
  for (const waiter of [...bridgeState.ackWaiters]) {
    if (bridgeState.lastAckedSequence >= waiter.targetSequence) {
      clearTimeout(waiter.failureTimer);
      bridgeState.ackWaiters.delete(waiter);
      waiter.resolve();
    }
  }
}

function rejectAllAckWaiters(bridgeState, reasonError) {
  for (const waiter of [...bridgeState.ackWaiters]) {
    clearTimeout(waiter.failureTimer);
    bridgeState.ackWaiters.delete(waiter);
    waiter.reject(reasonError);
  }
}

function clearPendingReadyWaiter(bridgeState, rejectionError = null) {
  const pendingWaiter = bridgeState.readyWaiter;
  if (!pendingWaiter) {
    return;
  }
  clearTimeout(pendingWaiter.failureTimer);
  bridgeState.readyWaiter = null;
  if (rejectionError) {
    pendingWaiter.reject(rejectionError);
  }
}

function handleSocketMessage(bridgeState, rawMessage) {
  const message = JSON.parse(String(rawMessage));
  if (message.type === 'terminal_ready') {
    bridgeState.isTerminalReady = true;
    bridgeState.initialTerminalState = message.state;
    const pendingWaiter = bridgeState.readyWaiter;
    clearPendingReadyWaiter(bridgeState);
    pendingWaiter?.resolve(message.state);
    return;
  }
  if (message.type === 'write_ack') {
    if (message.sequenceNumber > bridgeState.lastAckedSequence) {
      bridgeState.lastAckedSequence = message.sequenceNumber;
    }
    resolveSatisfiedAckWaiters(bridgeState);
    return;
  }
  if (message.type === 'term_input' && typeof message.data === 'string') {
    for (const listener of bridgeState.inputListeners) {
      listener(message.data);
    }
    return;
  }
  if (message.type === 'term_binary' && typeof message.binaryLatin1 === 'string') {
    const binaryBuffer = Buffer.from(message.binaryLatin1, 'latin1');
    for (const listener of bridgeState.inputListeners) {
      listener(binaryBuffer);
    }
  }
}

function waitForTerminalReadySignal(bridgeState, timeoutMs) {
  if (bridgeState.isTerminalReady) {
    return Promise.resolve(bridgeState.initialTerminalState);
  }
  clearPendingReadyWaiter(bridgeState);
  return new Promise((resolve, reject) => {
    const failureTimer = setTimeout(() => {
      bridgeState.readyWaiter = null;
      reject(new Error(`Timed out after ${timeoutMs}ms waiting for browser terminal_ready`));
    }, timeoutMs);
    bridgeState.readyWaiter = { failureTimer, resolve, reject };
  });
}

function flushPendingTerminalWrites(bridgeState, timeoutMs = 5000) {
  const targetSequence = bridgeState.lastSentSequence;
  if (bridgeState.lastAckedSequence >= targetSequence) {
    return Promise.resolve();
  }
  if (!bridgeState.isOpen || !bridgeState.activeSocket) {
    return Promise.reject(new Error('Cannot flush terminal writes: bridge socket is closed'));
  }
  return new Promise((resolve, reject) => {
    const waiter = {
      targetSequence,
      resolve,
      reject,
      failureTimer: setTimeout(() => {
        bridgeState.ackWaiters.delete(waiter);
        reject(
          new Error(
            `Timed out after ${timeoutMs}ms waiting for write_ack ${targetSequence} (last=${bridgeState.lastAckedSequence})`,
          ),
        );
      }, timeoutMs),
    };
    bridgeState.ackWaiters.add(waiter);
  });
}

function sendChunkOverBridge(bridgeState, chunk) {
  if (!bridgeState.isTerminalReady || !bridgeState.activeSocket) {
    throw new Error('Cannot write to browser terminal before terminal_ready');
  }
  bridgeState.lastSentSequence += 1;
  const sequenceNumber = bridgeState.lastSentSequence;
  bridgeState.activeSocket.send(
    JSON.stringify({
      type: 'pty_output',
      dataBase64: chunkToBase64(chunk),
      sequenceNumber,
    }),
  );
  return sequenceNumber;
}

async function launchChromiumPage(bridgeState, bridgeUrl, { headless, timeoutMs, executablePath }) {
  const readyPromise = waitForTerminalReadySignal(bridgeState, timeoutMs);
  readyPromise.catch(() => {});
  try {
    const launchOptions = executablePath ? { headless, executablePath } : { headless };
    bridgeState.browser = await chromium.launch(launchOptions);
    const browserContext = await bridgeState.browser.newContext({
      viewport: { width: 1280, height: 800 },
    });
    bridgeState.page = await browserContext.newPage();
    await bridgeState.page.goto(bridgeUrl);
  } catch (launchError) {
    clearPendingReadyWaiter(bridgeState);
    throw launchError;
  }
  return readyPromise;
}

async function teardownBridgeResources(bridgeState, httpServer, socketServer) {
  bridgeState.isOpen = false;
  bridgeState.isTerminalReady = false;
  clearPendingReadyWaiter(bridgeState, new Error('Bridge closed before terminal_ready'));
  rejectAllAckWaiters(bridgeState, new Error('Bridge closed before write_ack completed'));
  bridgeState.activeSocket?.close();
  bridgeState.activeSocket = null;
  for (const clientSocket of socketServer.clients) {
    clientSocket.terminate();
  }
  socketServer.close();
  if (bridgeState.browser) {
    await bridgeState.browser.close();
    bridgeState.browser = null;
  }
  await closeHttpServer(httpServer);
}

function createBridgeController(bridgeState, httpServer, socketServer, port, cols, rows) {
  const bridgeUrl = `http://127.0.0.1:${port}/?cols=${cols}&rows=${rows}`;
  return {
    port,
    url: bridgeUrl,
    get isTerminalReady() {
      return bridgeState.isTerminalReady;
    },
    get isOpen() {
      return bridgeState.isOpen && bridgeState.activeSocket !== null;
    },
    get initialTerminalState() {
      return bridgeState.initialTerminalState;
    },
    get page() {
      return bridgeState.page;
    },
    openBrowser: ({ headless = true, timeoutMs = 15000, executablePath } = {}) =>
      launchChromiumPage(bridgeState, bridgeUrl, { headless, timeoutMs, executablePath }),
    writeToTerminal: (chunk) => sendChunkOverBridge(bridgeState, chunk),
    flushTerminalWrites: (timeoutMs = 5000) => flushPendingTerminalWrites(bridgeState, timeoutMs),
    async writeAndFlush(chunk, timeoutMs = 5000) {
      sendChunkOverBridge(bridgeState, chunk);
      await flushPendingTerminalWrites(bridgeState, timeoutMs);
    },
    onTerminalInput(listener) {
      bridgeState.inputListeners.add(listener);
      return () => bridgeState.inputListeners.delete(listener);
    },
    async getTerminalState(timeoutMs = 5000) {
      await flushPendingTerminalWrites(bridgeState, timeoutMs);
      return bridgeState.page.evaluate(() => window.__getTerminalState());
    },
    clearOsc52Payloads: () => bridgeState.page.evaluate(() => window.__clearOsc52Payloads()),
    pasteText: (textToPaste) =>
      bridgeState.page.evaluate((value) => window.__pasteText(value), textToPaste),
    blurTerminal: () => bridgeState.page.evaluate(() => window.__blurTerminal()),
    focusTerminal: () => bridgeState.page.evaluate(() => window.__focusTerminal()),
    close: () => teardownBridgeResources(bridgeState, httpServer, socketServer),
  };
}

/**
 * Starts the local HTTP and WebSocket bridge server on `127.0.0.1:0` without launching Chromium yet.
 */
export async function createBridgeServer({ cols = 100, rows = 30 } = {}) {
  const httpServer = http.createServer(serveStaticAsset);
  const socketServer = new WebSocketServer({ server: httpServer, path: '/bridge' });
  const bridgeState = {
    isOpen: true,
    isTerminalReady: false,
    initialTerminalState: null,
    activeSocket: null,
    browser: null,
    page: null,
    readyWaiter: null,
    lastSentSequence: 0,
    lastAckedSequence: 0,
    ackWaiters: new Set(),
    inputListeners: new Set(),
  };

  socketServer.on('connection', (socket) => {
    bridgeState.activeSocket = socket;
    socket.on('message', (rawData) => handleSocketMessage(bridgeState, rawData));
    socket.on('close', () => {
      if (bridgeState.activeSocket === socket) {
        bridgeState.activeSocket = null;
        rejectAllAckWaiters(
          bridgeState,
          new Error('Browser terminal WebSocket closed unexpectedly.'),
        );
      }
    });
  });

  const port = await listenOnEphemeralLoopbackPort(httpServer);
  return createBridgeController(bridgeState, httpServer, socketServer, port, cols, rows);
}

/**
 * Starts the HTTP/WebSocket bridge, launches headless Playwright Chromium, and resolves
 * only after `@xterm/xterm@6.0.0` reports `terminal_ready` with non-zero cell metrics.
 */
export async function startBrowserBridge({
  cols = 100,
  rows = 30,
  headless = true,
  timeoutMs = 15000,
  executablePath,
} = {}) {
  const bridge = await createBridgeServer({ cols, rows });
  try {
    await bridge.openBrowser({ headless, timeoutMs, executablePath });
    return bridge;
  } catch (launchError) {
    await bridge.close();
    throw launchError;
  }
}
