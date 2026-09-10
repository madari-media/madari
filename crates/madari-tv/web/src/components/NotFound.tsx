import {useNavigate} from 'react-router-dom';
import {Button} from '@astryxdesign/core/Button';
import {Center} from '@astryxdesign/core/Center';
import {EmptyState} from '@astryxdesign/core/EmptyState';

/** Unknown URLs get a real answer instead of a silent redirect home. */
export function NotFound() {
  const navigate = useNavigate();
  return (
    <Center minHeight="60vh" padding={4}>
      <EmptyState
        title="That page does not exist"
        description="The link may be out of date. Settings live under a profile, and the remote is always one tap away."
        actions={
          <>
            <Button label="Go to profiles" variant="primary" onClick={() => navigate('/')} />
            <Button label="Open the remote" onClick={() => navigate('/remote')} />
          </>
        }
      />
    </Center>
  );
}
