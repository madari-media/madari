import {useEffect, useState} from 'react';
import {Button} from '@astryxdesign/core/Button';
import {HStack} from '@astryxdesign/core/HStack';
import {TextInput} from '@astryxdesign/core/TextInput';
import {VStack} from '@astryxdesign/core/VStack';
import {
  ArrowPathIcon,
  KeyIcon,
  LockClosedIcon,
  UserCircleIcon,
  UserPlusIcon,
} from '@heroicons/react/24/outline';
import type {ProfileDto} from '../client';
import {CONTROL_WIDTH, SettingsCard, SettingsRow} from './settings';

export function ProfilePanel({
  profile,
  canCreate,
  busy,
  onSave,
  onNew,
  onLock,
}: {
  profile: ProfileDto;
  canCreate: boolean;
  busy: boolean;
  onSave: (name: string, pin: string) => void;
  onNew: () => void;
  onLock: () => void;
}) {
  const [name, setName] = useState(profile.name);
  const [pin, setPin] = useState('');
  useEffect(() => {
    setName(profile.name);
    setPin('');
  }, [profile]);
  const dirty = name.trim() !== profile.name || pin.length > 0;

  return (
    <VStack gap={5}>
      <SettingsCard title="Profile">
        <SettingsRow
          title="Name"
          description="Shown on the profile picker and while watching."
          icon={UserCircleIcon}
          control={
            <TextInput
              label="Name"
              isLabelHidden
              size="sm"
              width={CONTROL_WIDTH}
              value={name}
              onChange={setName}
            />
          }
        />
        <SettingsRow
          title="PIN"
          description={
            profile.kids
              ? 'Kids profiles use their guardian’s PIN.'
              : '4–8 digits. Leave empty to keep the current PIN.'
          }
          icon={KeyIcon}
          control={
            <TextInput
              label="New PIN"
              isLabelHidden
              size="sm"
              type="password"
              width={CONTROL_WIDTH}
              value={pin}
              isDisabled={profile.kids}
              onChange={setPin}
            />
          }
        />
        <SettingsRow
          title="Save changes"
          description={dirty ? 'Your edits are not on the TV yet.' : 'Everything is saved.'}
          icon={ArrowPathIcon}
          control={
            <HStack gap={1} align="center">
              <Button
                label="Discard"
                variant="ghost"
                size="sm"
                isDisabled={!dirty}
                onClick={() => {
                  setName(profile.name);
                  setPin('');
                }}
              />
              <Button
                label="Save profile"
                variant="primary"
                size="sm"
                isLoading={busy}
                isDisabled={!name.trim() || !dirty}
                onClick={() => onSave(name.trim(), pin)}
              />
            </HStack>
          }
        />
      </SettingsCard>

      <SettingsCard title="Access">
        <SettingsRow
          title="New profile"
          description="Add another person, or a kids profile with a guardian PIN."
          icon={UserPlusIcon}
          control={
            <Button
              label="New profile"
              variant="secondary"
              size="sm"
              isDisabled={!canCreate || busy}
              onClick={onNew}
            />
          }
        />
        <SettingsRow
          title="Lock this profile"
          description="Return the TV to the profile picker and close this session."
          icon={LockClosedIcon}
          control={
            <Button label="Lock" variant="secondary" size="sm" isDisabled={busy} onClick={onLock} />
          }
        />
      </SettingsCard>
    </VStack>
  );
}
