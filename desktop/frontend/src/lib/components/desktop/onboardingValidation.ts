// SPDX-License-Identifier: AGPL-3.0-only
import { get, writable } from 'svelte/store';

import { listCanvasCourses, validateVisionModel } from '$lib/desktop';

const CONNECTION_MESSAGE_VISIBLE_MS = 1500;

export type ConnectionStatus = 'idle' | 'checking' | 'ok' | 'error';
export type ConnectionMessageTone = 'warning' | 'info' | 'success' | 'error';

export type ConnectionTickerMessage = {
  id: number;
  text: string;
  tone: ConnectionMessageTone;
  sticky: boolean;
};

type ValidationState = {
  connectionStatus: ConnectionStatus;
  validatedCredentialsKey: string | null;
  displayedMessage: ConnectionTickerMessage | null;
};

type OllamaProvider = 'ollama_native' | 'ollama_cloud';

const initialState: ValidationState = {
  connectionStatus: 'idle',
  validatedCredentialsKey: null,
  displayedMessage: null
};

function isValidHttpUrl(url: string) {
  try {
    const parsed = new URL(url);
    return parsed.protocol === 'https:' || parsed.protocol === 'http:';
  } catch {
    return false;
  }
}

function connectionMessageTone(status: ConnectionStatus): ConnectionMessageTone {
  switch (status) {
    case 'ok':
      return 'success';
    case 'checking':
      return 'info';
    case 'error':
      return 'error';
    default:
      return 'warning';
  }
}

function canvasCredentialIdleMessage(url: string, token: string, hasDesktopHost: boolean) {
  if (url.length === 0 || token.length === 0) {
    return 'Enter a Canvas base URL and API token to continue.';
  }
  if (!isValidHttpUrl(url)) {
    return 'Enter a valid Canvas base URL.';
  }
  if (!hasDesktopHost) {
    return 'Connection check runs in the ScriptScore desktop app.';
  }
  return null;
}

function ollamaCredentialIdleMessage(
  provider: OllamaProvider,
  baseUrl: string,
  model: string,
  apiKey: string,
  hasDesktopHost: boolean
) {
  if (baseUrl.length === 0) {
    return 'Enter an Ollama URL to continue.';
  }
  if (model.trim().length === 0) {
    return 'Enter an Ollama vision model to continue.';
  }
  if (provider === 'ollama_cloud' && apiKey.length === 0) {
    return 'Enter an Ollama Cloud API key to continue.';
  }
  if (!isValidHttpUrl(baseUrl)) {
    return 'Enter a valid Ollama URL.';
  }
  if (!hasDesktopHost) {
    return 'Connection check runs in the ScriptScore desktop app.';
  }
  return null;
}

function createTicker(store: ReturnType<typeof writable<ValidationState>>) {
  let id = 0;
  let queue: ConnectionTickerMessage[] = [];
  let timer: ReturnType<typeof setTimeout> | null = null;

  function clearTimer() {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
  }

  function showNext() {
    clearTimer();
    const [next, ...rest] = queue;
    queue = rest;
    store.update((state) => ({ ...state, displayedMessage: next ?? null }));
    if (!next || next.sticky) {
      return;
    }
    timer = setTimeout(() => {
      timer = null;
      showNext();
    }, CONNECTION_MESSAGE_VISIBLE_MS);
  }

  return {
    clear() {
      clearTimer();
      queue = [];
      store.update((state) => ({ ...state, displayedMessage: null }));
    },
    destroy: clearTimer,
    push(status: ConnectionStatus, text: string) {
      if (!text) {
        return;
      }
      const current = get(store);
      if (current.displayedMessage?.sticky) {
        this.clear();
      }
      queue = [
        ...queue,
        {
          id: ++id,
          text,
          tone: connectionMessageTone(status),
          sticky: status === 'error'
        }
      ];
      if (!get(store).displayedMessage) {
        showNext();
      }
    }
  };
}

function createBaseValidationController() {
  const store = writable<ValidationState>(initialState);
  const ticker = createTicker(store);
  let validateTimer: ReturnType<typeof setTimeout> | null = null;
  let previousCredentialsKey: string | null = null;
  let validateGeneration = 0;

  function clearValidateTimer() {
    if (validateTimer) {
      clearTimeout(validateTimer);
      validateTimer = null;
    }
  }

  function reset() {
    clearValidateTimer();
    ticker.clear();
    previousCredentialsKey = null;
    store.set(initialState);
  }

  function setConnection(status: ConnectionStatus, message: string) {
    store.update((state) => ({ ...state, connectionStatus: status }));
    ticker.push(status, message);
  }

  return {
    subscribe: store.subscribe,
    clearValidateTimer,
    reset,
    schedule(run: () => void) {
      clearValidateTimer();
      validateTimer = setTimeout(() => {
        validateTimer = null;
        run();
      }, 300);
    },
    setIdle(credentialsKey: string) {
      clearValidateTimer();
      previousCredentialsKey = credentialsKey;
      store.update((state) => ({
        ...state,
        connectionStatus: 'idle',
        validatedCredentialsKey: null
      }));
      ticker.clear();
    },
    shouldValidate(credentialsKey: string) {
      if (credentialsKey === previousCredentialsKey) {
        return false;
      }
      previousCredentialsKey = credentialsKey;
      store.update((state) => ({ ...state, validatedCredentialsKey: null }));
      ticker.clear();
      return true;
    },
    nextGeneration() {
      validateGeneration += 1;
      return validateGeneration;
    },
    isCurrentGeneration(gen: number) {
      return gen === validateGeneration;
    },
    setValidated(credentialsKey: string) {
      store.update((state) => ({ ...state, validatedCredentialsKey: credentialsKey }));
    },
    clearValidated() {
      store.update((state) => ({ ...state, validatedCredentialsKey: null }));
    },
    setConnection,
    destroy() {
      clearValidateTimer();
      ticker.destroy();
    }
  };
}

export function createCanvasOnboardingValidator(hasDesktopHost: () => boolean) {
  const controller = createBaseValidationController();

  async function runValidation(url: string, token: string, credentialsKey: string) {
    const gen = controller.nextGeneration();
    const idleMessage = canvasCredentialIdleMessage(url, token, hasDesktopHost());
    if (idleMessage) {
      if (controller.isCurrentGeneration(gen)) {
        controller.clearValidated();
      }
      return;
    }

    controller.setConnection('checking', 'Checking Canvas connection...');
    try {
      const courses = await listCanvasCourses(url, token);
      if (!controller.isCurrentGeneration(gen)) {
        return;
      }
      controller.setValidated(credentialsKey);
      controller.setConnection(
        'ok',
        courses.length === 1
          ? 'Connected. 1 course available.'
          : `Connected. ${courses.length} courses available.`
      );
    } catch (error) {
      if (!controller.isCurrentGeneration(gen)) {
        return;
      }
      controller.clearValidated();
      controller.setConnection('error', String(error));
    }
  }

  return {
    subscribe: controller.subscribe,
    destroy: controller.destroy,
    handle(provider: 'none' | 'canvas', url: string, token: string) {
      if (provider !== 'canvas') {
        controller.reset();
        return;
      }

      const credentialsKey = `${url}|${token}`;
      if (canvasCredentialIdleMessage(url, token, hasDesktopHost())) {
        controller.setIdle(credentialsKey);
        return;
      }

      if (!hasDesktopHost()) {
        controller.reset();
        return;
      }

      if (controller.shouldValidate(credentialsKey)) {
        controller.schedule(() => {
          void runValidation(url, token, credentialsKey);
        });
      }
    }
  };
}

export function createOllamaOnboardingValidator(hasDesktopHost: () => boolean) {
  const controller = createBaseValidationController();

  async function runValidation(
    provider: OllamaProvider,
    baseUrl: string,
    model: string,
    apiKey: string,
    credentialsKey: string
  ) {
    const gen = controller.nextGeneration();
    const idleMessage = ollamaCredentialIdleMessage(provider, baseUrl, model, apiKey, hasDesktopHost());
    if (idleMessage) {
      if (controller.isCurrentGeneration(gen)) {
        controller.clearValidated();
      }
      return;
    }

    controller.setConnection('checking', 'Checking Ollama connection...');
    try {
      const validation = await validateVisionModel(
        provider,
        baseUrl,
        model,
        provider === 'ollama_cloud' ? apiKey : null
      );
      if (!controller.isCurrentGeneration(gen)) {
        return;
      }
      controller.setValidated(credentialsKey);
      if (validation.valid) {
        controller.setConnection('ok', `Connected. ${validation.displayName} supports vision.`);
      } else {
        controller.clearValidated();
        controller.setConnection(
          'error',
          validation.reason === 'missing_capabilities'
            ? `${validation.displayName} does not support vision.`
            : `${validation.displayName} is unavailable.`
        );
      }
    } catch (error) {
      if (!controller.isCurrentGeneration(gen)) {
        return;
      }
      controller.clearValidated();
      controller.setConnection('error', String(error));
    }
  }

  return {
    subscribe: controller.subscribe,
    destroy: controller.destroy,
    handle(
      aiAssist: boolean,
      provider: string,
      baseUrl: string,
      model: string,
      apiKey: string
    ) {
      if (!aiAssist || (provider !== 'ollama_native' && provider !== 'ollama_cloud')) {
        controller.reset();
        return;
      }

      const credentialsKey =
        provider === 'ollama_cloud'
          ? `${provider}|${baseUrl}|${model}|${apiKey}`
          : `${provider}|${baseUrl}|${model}`;
      if (ollamaCredentialIdleMessage(provider, baseUrl, model, apiKey, hasDesktopHost())) {
        controller.setIdle(credentialsKey);
        return;
      }

      if (!hasDesktopHost()) {
        controller.reset();
        return;
      }

      if (controller.shouldValidate(credentialsKey)) {
        controller.schedule(() => {
          void runValidation(provider, baseUrl, model, apiKey, credentialsKey);
        });
      }
    }
  };
}
