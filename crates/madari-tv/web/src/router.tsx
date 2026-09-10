import {Outlet, createBrowserRouter, useOutletContext} from 'react-router-dom';
import {Button} from '@astryxdesign/core/Button';
import {Center} from '@astryxdesign/core/Center';
import {EmptyState} from '@astryxdesign/core/EmptyState';
import {useController, type Controller} from './useController';
import {Pairing} from './components/Pairing';
import {Dashboard} from './components/Dashboard';
import {Loading} from './components/Loading';
import {NotFound} from './components/NotFound';
import {RemoteProvider} from './components/Remote';

/**
 * Root route. It owns the three states that gate every screen — pairing, first
 * load, and an unreachable TV — and hands the controller to its children
 * through the router outlet context rather than prop drilling. The floating
 * remote sits outside the outlet so it stays reachable on every screen.
 */
function Root() {
  const controller = useController();
  if (!controller.paired) return <Pairing controller={controller} />;
  if (controller.isLoading && !controller.overview) return <Loading label="Reading your TV…" />;
  // Paired but unreachable: a non-401 failure leaves the stored session intact.
  if (!controller.overview) {
    return (
      <Center minHeight="60vh" padding={4}>
        <EmptyState
          title="Can't reach your TV"
          description={controller.error ?? 'The TV did not answer. Keep Madari open and check your Wi-Fi.'}
          actions={
            <>
              <Button label="Try again" variant="primary" onClick={controller.refresh} />
              <Button
                label="Pair again"
                onClick={() => {
                  controller.disconnect().catch(() => {});
                }}
              />
            </>
          }
        />
      </Center>
    );
  }
  return (
    <RemoteProvider
      disabled={false}
      onCommand={(command) => {
        controller.sendRemote(command).catch(() => {});
      }}
    >
      <Outlet context={controller} />
    </RemoteProvider>
  );
}

/** Typed access to the controller the root route provides. */
export function useControllerContext(): Controller {
  return useOutletContext<Controller>();
}

export const router = createBrowserRouter([
  {
    path: '/',
    element: <Root />,
    children: [
      {index: true, element: <Dashboard />},
      {path: 'manage/:section', element: <Dashboard />},
      {path: '*', element: <NotFound />},
    ],
  },
]);
