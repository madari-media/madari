import {Center} from '@astryxdesign/core/Center';
import {Spinner} from '@astryxdesign/core/Spinner';
import {Text} from '@astryxdesign/core/Text';
import {VStack} from '@astryxdesign/core/VStack';

export function Loading({label = 'Loading…'}: {label?: string}) {
  return (
    <Center minHeight="60vh">
      <VStack gap={3} hAlign="center">
        <Spinner size="xl" />
        <Text type="supporting" color="secondary">
          {label}
        </Text>
      </VStack>
    </Center>
  );
}
