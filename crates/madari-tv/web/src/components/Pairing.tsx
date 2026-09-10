import {useState} from 'react';
import {Banner} from '@astryxdesign/core/Banner';
import {Button} from '@astryxdesign/core/Button';
import {Card} from '@astryxdesign/core/Card';
import {Center} from '@astryxdesign/core/Center';
import {FormLayout} from '@astryxdesign/core/FormLayout';
import {Heading, Text} from '@astryxdesign/core/Text';
import {TextInput} from '@astryxdesign/core/TextInput';
import {VStack} from '@astryxdesign/core/VStack';
import type {Controller} from '../useController';

/** Zero-state entry surface: one field, one action, nothing competing. */
export function Pairing({controller}: {controller: Controller}) {
  const [code, setCode] = useState('');
  const [localError, setLocalError] = useState<string | null>(null);
  const value = code.trim().toUpperCase();

  function submit() {
    if (value.length !== 8) {
      setLocalError('The code is eight characters, like A1B2C3D4.');
      return;
    }
    setLocalError(null);
    controller.pair(value).catch(() => {
      // The controller surfaces the server message in the banner.
    });
  }

  const problem = localError ?? controller.error;

  return (
    <Center minHeight="100dvh" padding={4}>
      <Card width="min(520px, 100%)" padding={6}>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            submit();
          }}
        >
          <VStack gap={5}>
            <VStack gap={1}>
              <Text type="supporting" color="secondary">
                TV settings
              </Text>
              <Heading level={1}>Your TV. Your setup.</Heading>
              <Text color="secondary">
                Manage addons, profiles and playback from this browser. Open Settings, then Web settings, on your TV
                to read the pairing code.
              </Text>
            </VStack>
            {problem ? <Banner status="error" title={problem} /> : null}
            <FormLayout>
              <TextInput
                label="TV pairing code"
                value={value}
                onChange={(next) => {
                  setCode(next.toUpperCase().slice(0, 8));
                  setLocalError(null);
                }}
                onEnter={submit}
                placeholder="A1B2C3D4"
                isRequired
                hasAutoFocus
              />
            </FormLayout>
            <Button
              label="Connect to TV"
              variant="primary"
              type="submit"
              width="100%"
              isLoading={controller.busy}
              isDisabled={value.length !== 8}
            />
            <Text type="supporting" color="secondary">
              Keep Madari open on the TV. This browser stays paired until you disconnect it.
            </Text>
          </VStack>
        </form>
      </Card>
    </Center>
  );
}
