import '@testing-library/jest-dom';
import { afterAll, afterEach, beforeAll, beforeEach } from 'vitest';
import { cleanup } from '@testing-library/react';
import { server } from './mocks/server';

/**
 * MSW Server Lifecycle
 *
 * Start the server before all tests, reset handlers after each test,
 * and close the server after all tests complete.
 */
beforeAll(() => {
  server.listen({
    onUnhandledRequest: 'warn', // Warn about unhandled requests
  });
});

afterAll(() => {
  server.close();
});

beforeEach(() => {
  localStorage.clear();
});

afterEach(() => {
  cleanup();
  server.resetHandlers(); // Reset to default handlers after each test
});
