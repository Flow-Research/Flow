/**
 * MSW Server Setup for Node.js (Vitest)
 *
 * This creates a mock server that intercepts all network requests
 * during tests, allowing us to test real fetch calls without mocking modules.
 */

import { setupServer } from 'msw/node';
import { handlers } from './handlers';

// Create the server with default handlers
export const server = setupServer(...handlers);
