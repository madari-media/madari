import {defineConfig} from '@hey-api/openapi-ts';

// The spec is generated from the Rust handlers:
//   npm run openapi   (cargo run -p madari-tv --features openapi --example web_openapi)
// Then regenerate the client with:
//   npm run codegen
export default defineConfig({
  input: 'openapi.json',
  output: {
    path: 'src/client',
    postProcess: [],
  },
  plugins: [
    {
      name: '@hey-api/client-fetch',
      runtimeConfigPath: './src/hey-api.ts',
    },
    '@tanstack/react-query',
  ],
});
