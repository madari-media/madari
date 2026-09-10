import {StrictMode} from 'react';
import {createRoot} from 'react-dom/client';
import {QueryClient, QueryClientProvider} from '@tanstack/react-query';
import {RouterProvider} from 'react-router-dom';
import '@astryxdesign/core/reset.css';
// Component styles are compiled from source by astryxStylex(); the prebuilt
// @astryxdesign/core/astryx.css bundle would duplicate them.
import {Theme} from '@astryxdesign/core/theme';
import {ToastViewport} from '@astryxdesign/core/Toast';
import {madariTheme} from './theme/madari';
import './theme/madari.css';
import './index.css';
import {configureAuth} from './auth';
import {router} from './router';

configureAuth();

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: false,
      refetchOnWindowFocus: false,
      staleTime: 5_000,
    },
  },
});

const container = document.getElementById('root');
if (!container) {
  throw new Error('Missing #root element');
}

createRoot(container).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <Theme theme={madariTheme}>
        <ToastViewport position="bottomEnd" maxVisible={3}>
          <RouterProvider router={router} />
        </ToastViewport>
      </Theme>
    </QueryClientProvider>
  </StrictMode>,
);
