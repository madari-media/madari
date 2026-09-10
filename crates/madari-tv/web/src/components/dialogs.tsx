import type {ReactNode} from 'react';
import {useEffect, useState} from 'react';
import {Button} from '@astryxdesign/core/Button';
import {Dialog} from '@astryxdesign/core/Dialog';
import {FormLayout} from '@astryxdesign/core/FormLayout';
import {HStack} from '@astryxdesign/core/HStack';
import {RadioList, RadioListItem} from '@astryxdesign/core/RadioList';
import {Switch} from '@astryxdesign/core/Switch';
import {Heading, Text} from '@astryxdesign/core/Text';
import {TextInput} from '@astryxdesign/core/TextInput';
import {VStack} from '@astryxdesign/core/VStack';
import type {AddonSummary, ProfileDto} from '../client';

/** Shared dialog chrome: heading, supporting copy, fields, then a right-aligned action row. */
function FormDialog({
  open,
  purpose,
  width,
  title,
  description,
  busy,
  submitLabel,
  submitDisabled,
  onClose,
  onSubmit,
  children,
}: {
  open: boolean;
  purpose: 'form' | 'required' | 'info';
  width: number;
  title: string;
  description: string;
  busy: boolean;
  submitLabel: string;
  submitDisabled?: boolean;
  onClose: () => void;
  onSubmit: () => void;
  children?: ReactNode;
}) {
  return (
    <Dialog isOpen={open} onOpenChange={(next) => !next && onClose()} purpose={purpose} width={width}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          onSubmit();
        }}
      >
        <VStack gap={5}>
          <VStack gap={1}>
            <Heading level={3}>{title}</Heading>
            <Text color="secondary">{description}</Text>
          </VStack>
          {children}
          <HStack gap={2} hAlign="end">
            <Button label="Cancel" type="button" onClick={onClose} />
            <Button
              label={submitLabel}
              variant="primary"
              type="submit"
              isLoading={busy}
              isDisabled={submitDisabled}
            />
          </HStack>
        </VStack>
      </form>
    </Dialog>
  );
}

/** Install a new addon or reconfigure an existing (shared) installation. */
export function AddonDialog({
  addon,
  open,
  busy,
  onClose,
  onSubmit,
}: {
  addon: AddonSummary | null;
  open: boolean;
  busy: boolean;
  onClose: () => void;
  onSubmit: (url: string, allowLocal: boolean) => void;
}) {
  const [url, setUrl] = useState('');
  const [allowLocal, setAllowLocal] = useState(false);
  useEffect(() => {
    if (!open) return;
    setUrl('');
    setAllowLocal(addon?.allow_local ?? false);
  }, [open, addon]);
  return (
    <FormDialog
      open={open}
      purpose="form"
      width={560}
      title={addon ? `Reconfigure ${addon.manifest.name}` : 'Install addon'}
      description={
        addon
          ? 'Paste the new configured manifest URL. Every profile sharing this installation receives the change.'
          : 'Paste the complete configured manifest URL from your addon provider.'
      }
      busy={busy}
      submitLabel={addon ? 'Save configuration' : 'Install addon'}
      submitDisabled={!url.trim()}
      onClose={onClose}
      onSubmit={() => onSubmit(url.trim(), allowLocal)}
    >
      <VStack gap={4}>
        <FormLayout>
          <TextInput
            label="Manifest URL"
            value={url}
            onChange={setUrl}
            placeholder="https://addon.example/manifest.json"
            isRequired
            hasAutoFocus
          />
        </FormLayout>
        <Switch
          label="Allow access to a local-network addon"
          description="Only enable this for a server you run on your own network."
          value={allowLocal}
          onChange={setAllowLocal}
        />
      </VStack>
    </FormDialog>
  );
}

/** Link one installation to another profile; its PIN (or guardian PIN) is required. */
export function ShareDialog({
  addon,
  targets,
  open,
  busy,
  onClose,
  onSubmit,
}: {
  addon: AddonSummary | null;
  targets: ProfileDto[];
  open: boolean;
  busy: boolean;
  onClose: () => void;
  onSubmit: (targetId: string, pin: string) => void;
}) {
  const [target, setTarget] = useState('');
  const [pin, setPin] = useState('');
  useEffect(() => {
    if (!open) return;
    setTarget(targets[0]?.id ?? '');
    setPin('');
  }, [open, targets]);
  return (
    <FormDialog
      open={open}
      purpose="form"
      width={560}
      title={addon ? `Share ${addon.manifest.name}` : 'Share addon'}
      description="This links one installation. Configuration is shared; enabled status, order, library and progress stay separate."
      busy={busy}
      submitLabel="Share addon"
      submitDisabled={!target}
      onClose={onClose}
      onSubmit={() => onSubmit(target, pin)}
    >
      {targets.length === 0 ? (
        <Text type="supporting" color="secondary">
          Create another profile first.
        </Text>
      ) : (
        <VStack gap={4}>
          <RadioList label="Share with" value={target} onChange={setTarget}>
            {targets.map((profile) => (
              <RadioListItem
                key={profile.id}
                value={profile.id}
                label={profile.name}
                description={
                  profile.kids
                    ? 'Kids profile · guardian PIN'
                    : profile.pin_protected
                      ? 'PIN protected'
                      : 'No PIN'
                }
              />
            ))}
          </RadioList>
          <FormLayout>
            <TextInput
              label="Recipient or guardian PIN"
              description="Leave empty when sharing to your own kids profile or a profile without a PIN."
              type="password"
              value={pin}
              onChange={setPin}
              isOptional
            />
          </FormLayout>
        </VStack>
      )}
    </FormDialog>
  );
}

/** Destructive confirmation, e.g. removing an addon from a profile. */
export function ConfirmDialog({
  open,
  title,
  body,
  confirmLabel,
  busy,
  onClose,
  onConfirm,
}: {
  open: boolean;
  title: string;
  body: string;
  confirmLabel: string;
  busy: boolean;
  onClose: () => void;
  onConfirm: () => void;
}) {
  return (
    <Dialog isOpen={open} onOpenChange={(next) => !next && onClose()} purpose="required" width={480}>
      <VStack gap={5}>
        <VStack gap={1}>
          <Heading level={3}>{title}</Heading>
          <Text color="secondary">{body}</Text>
        </VStack>
        <HStack gap={2} hAlign="end">
          <Button label="Cancel" type="button" onClick={onClose} />
          <Button label={confirmLabel} variant="destructive" isLoading={busy} onClick={onConfirm} />
        </HStack>
      </VStack>
    </Dialog>
  );
}

/**
 * Open a PIN-protected profile for management. Profiles without a PIN are opened
 * straight from the profile card, so this dialog never asks for one.
 */
export function SelectProfileDialog({
  profile,
  busy,
  onClose,
  onSubmit,
}: {
  profile: ProfileDto | null;
  busy: boolean;
  onClose: () => void;
  onSubmit: (pin: string) => void;
}) {
  const [pin, setPin] = useState('');
  useEffect(() => {
    if (profile) setPin('');
  }, [profile]);
  return (
    <FormDialog
      open={profile !== null}
      purpose="form"
      width={480}
      title={`Open ${profile?.name ?? ''}`}
      description={
        profile?.kids
          ? "Enter the guardian's PIN to manage this kids profile."
          : 'Enter the profile PIN to manage it.'
      }
      busy={busy}
      submitLabel="Open settings"
      submitDisabled={pin.length === 0}
      onClose={onClose}
      onSubmit={() => onSubmit(pin)}
    >
      <FormLayout>
        <TextInput
          label={profile?.kids ? 'Guardian PIN' : 'Profile PIN'}
          type="password"
          value={pin}
          onChange={setPin}
          isRequired
          hasAutoFocus
          onEnter={() => onSubmit(pin)}
        />
      </FormLayout>
    </FormDialog>
  );
}

/** Create a regular or kids profile. Kids profiles need a PIN-protected adult. */
export function NewProfileDialog({
  open,
  hasGuardian,
  busy,
  onClose,
  onSubmit,
}: {
  open: boolean;
  hasGuardian: boolean;
  busy: boolean;
  onClose: () => void;
  onSubmit: (name: string, pin: string, kids: boolean) => void;
}) {
  const [name, setName] = useState('');
  const [pin, setPin] = useState('');
  const [kids, setKids] = useState(false);
  useEffect(() => {
    if (!open) return;
    setName('');
    setPin('');
    setKids(false);
  }, [open]);
  return (
    <FormDialog
      open={open}
      purpose="form"
      width={520}
      title="Create a profile"
      description={
        hasGuardian
          ? 'A profile with a PIN protects settings and can be a guardian for kids profiles.'
          : 'Create your first regular profile, then give it a PIN to protect settings.'
      }
      busy={busy}
      submitLabel="Create profile"
      submitDisabled={!name.trim()}
      onClose={onClose}
      onSubmit={() => onSubmit(name.trim(), pin, kids)}
    >
      <VStack gap={4}>
        <FormLayout>
          <TextInput label="Name" value={name} onChange={setName} isRequired hasAutoFocus />
          <TextInput
            label="PIN"
            description="4–8 digits. Leave empty for no PIN."
            type="password"
            value={pin}
            onChange={setPin}
            isOptional
          />
        </FormLayout>
        {hasGuardian ? (
          <Switch
            label="Kids profile"
            description="Kids profiles use the guardian's PIN and the guardian's chosen addons."
            value={kids}
            onChange={setKids}
          />
        ) : null}
      </VStack>
    </FormDialog>
  );
}
