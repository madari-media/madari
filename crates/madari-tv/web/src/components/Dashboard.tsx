import {useEffect, useState} from 'react';
import {useNavigate, useParams} from 'react-router-dom';
import {AppShell} from '@astryxdesign/core/AppShell';
import {Avatar} from '@astryxdesign/core/Avatar';
import {Banner} from '@astryxdesign/core/Banner';
import {Button} from '@astryxdesign/core/Button';
import {ClickableCard} from '@astryxdesign/core/ClickableCard';
import {Grid} from '@astryxdesign/core/Grid';
import {HStack} from '@astryxdesign/core/HStack';
import {Icon} from '@astryxdesign/core/Icon';
import {Layout, LayoutContent} from '@astryxdesign/core/Layout';
import {Section} from '@astryxdesign/core/Section';
import {SideNav, SideNavHeading, SideNavItem, SideNavSection} from '@astryxdesign/core/SideNav';
import {StackItem} from '@astryxdesign/core/Stack';
import {Toolbar} from '@astryxdesign/core/Toolbar';
import {TopNav, TopNavHeading} from '@astryxdesign/core/TopNav';
import {Heading, Text} from '@astryxdesign/core/Text';
import {VStack} from '@astryxdesign/core/VStack';
import {
  AdjustmentsHorizontalIcon,
  PuzzlePieceIcon,
  SignalIcon,
  UserCircleIcon,
} from '@heroicons/react/24/outline';
import type {AddonSummary, ProfileDto} from '../client';
import {normalizePreferences} from '../preferences';
import {useControllerContext} from '../router';
import {useRemote} from './Remote';
import {AddonsPanel} from './AddonsPanel';
import {Loading} from './Loading';
import {PlaybackPanel} from './PlaybackPanel';
import {ProfilePanel} from './ProfilePanel';
import {SettingsPanel} from './settings';
import {AddonDialog, ConfirmDialog, NewProfileDialog, SelectProfileDialog, ShareDialog} from './dialogs';

const SECTIONS = [
  {
    value: 'addons',
    label: 'Addons',
    description: 'Sources, order and sharing',
    icon: PuzzlePieceIcon,
  },
  {
    value: 'playback',
    label: 'Playback',
    description: 'Audio and subtitle preferences',
    icon: AdjustmentsHorizontalIcon,
  },
  {
    value: 'profile',
    label: 'Profile',
    description: 'Name, PIN and access',
    icon: UserCircleIcon,
  },
] as const;
type SectionKey = (typeof SECTIONS)[number]['value'];

function isSection(value: string | undefined): value is SectionKey {
  return value !== undefined && SECTIONS.some((entry) => entry.value === value);
}

function profileSubtitle(profile: ProfileDto): string {
  if (profile.kids) return 'Kids · guardian PIN';
  if (profile.pin_protected) return 'PIN protected';
  return 'No PIN';
}

export function Dashboard() {
  const controller = useControllerContext();
  const remote = useRemote();
  const overview = controller.overview;
  const {section} = useParams();
  const navigate = useNavigate();
  const [selecting, setSelecting] = useState<ProfileDto | null>(null);
  const [installing, setInstalling] = useState(false);
  const [configuring, setConfiguring] = useState<AddonSummary | null>(null);
  const [sharing, setSharing] = useState<AddonSummary | null>(null);
  const [removing, setRemoving] = useState<AddonSummary | null>(null);
  const [creating, setCreating] = useState(false);

  const profiles = overview?.profiles ?? [];
  const selected = overview?.selected ?? null;
  const snapshot = overview?.snapshot ?? null;
  const addons = snapshot?.addons ?? [];
  const tab: SectionKey = isSection(section) ? section : 'addons';
  const managing = section !== undefined;

  // A deep link only means something while a profile is open on the TV's session.
  useEffect(() => {
    if (managing && overview && !selected) navigate('/', {replace: true});
  }, [managing, overview, selected, navigate]);

  if (!overview) return <Loading label="Reading your TV…" />;

  const targets = selected ? profiles.filter((profile) => profile.id !== selected.id) : [];
  const canCreate = profiles.length === 0 || selected !== null;
  const current = SECTIONS.find((entry) => entry.value === tab) ?? SECTIONS[0];
  const open = managing && selected !== null && snapshot !== null;

  /** A profile without a PIN opens straight away; only protected ones prompt. */
  function openProfile(profile: ProfileDto) {
    if (selected?.id === profile.id) {
      navigate('/manage/addons');
      return;
    }
    if (!profile.pin_protected && !profile.kids) {
      controller
        .selectProfile(profile.id, '')
        .then(() => navigate('/manage/addons'))
        .catch(() => {});
      return;
    }
    setSelecting(profile);
  }

  function moveAddon(index: number, delta: number) {
    const ids = addons.map((addon) => addon.installation_id);
    const target = index + delta;
    if (target < 0 || target >= ids.length) return;
    [ids[index], ids[target]] = [ids[target], ids[index]];
    controller.reorderAddons(ids).catch(() => {});
  }

  const panel =
    tab === 'addons' ? (
      <AddonsPanel
        addons={addons}
        busy={controller.busy}
        disabled={false}
        onInstall={() => setInstalling(true)}
        onToggle={(addon) => {
          controller.setAddonEnabled(addon.installation_id, !addon.enabled).catch(() => {});
        }}
        onMove={moveAddon}
        onConfigure={setConfiguring}
        onShare={setSharing}
        onRemove={setRemoving}
      />
    ) : tab === 'playback' && snapshot ? (
      <PlaybackPanel
        preferences={normalizePreferences(snapshot.playback_preferences)}
        busy={controller.busy}
        onSave={(next) => {
          controller.setPreferences(next).catch(() => {});
        }}
      />
    ) : selected ? (
      <ProfilePanel
        profile={selected}
        canCreate={canCreate}
        busy={controller.busy}
        onSave={(name, pin) => {
          controller.updateProfile(name, pin).catch(() => {});
        }}
        onNew={() => setCreating(true)}
        onLock={() => {
          controller
            .lockProfile()
            .then(() => navigate('/'))
            .catch(() => {});
        }}
      />
    ) : null;

  return (
    <AppShell
      height="auto"
      contentPadding={open ? 4 : 0}
      variant="surface"
      topNav={
        <TopNav
          label="Madari TV settings"
          heading={<TopNavHeading heading="madari" subheading="TV settings" />}
          endContent={
            <HStack gap={2} align="center">
              <Button
                label="Remote"
                size="sm"
                variant="secondary"
                icon={<Icon icon={SignalIcon} size="sm" />}
                tooltip="TV remote"
                onClick={remote.toggle}
              />
              <Button label="Refresh" size="sm" variant="ghost" onClick={controller.refresh} />
              <Button
                label="Disconnect"
                size="sm"
                variant="ghost"
                isLoading={controller.busy}
                onClick={() => {
                  controller.disconnect().catch(() => {});
                }}
              />
            </HStack>
          }
        />
      }
      sideNav={
        open && selected ? (
          <SideNav
            aria-label="Settings sections"
            header={<SideNavHeading heading={selected.name} subheading={profileSubtitle(selected)} />}
            topContent={
              <Button
                label="All profiles"
                variant="secondary"
                width="100%"
                onClick={() => navigate('/')}
              />
            }
          >
            <SideNavSection title="Settings">
              {SECTIONS.map((entry) => (
                <SideNavItem
                  key={entry.value}
                  label={entry.label}
                  icon={<Icon icon={entry.icon} size="sm" color="primary" />}
                  isSelected={tab === entry.value}
                  onClick={() => navigate(`/manage/${entry.value}`)}
                />
              ))}
            </SideNavSection>
          </SideNav>
        ) : undefined
      }
    >
      {open && selected && snapshot ? (
        <VStack gap={5}>
          <HStack gap={3} align="start">
            <StackItem size="fill">
              <VStack gap={0.5}>
                <Heading level={2}>{current.label}</Heading>
                <Text type="supporting" color="secondary">
                  {current.description}
                </Text>
              </VStack>
            </StackItem>
            {tab === 'addons' ? (
              <Button label="Install addon" variant="primary" onClick={() => setInstalling(true)} />
            ) : null}
            {tab === 'profile' ? (
              <Button
                label="New profile"
                variant="primary"
                isDisabled={!canCreate}
                onClick={() => setCreating(true)}
              />
            ) : null}
          </HStack>
          <SettingsPanel>{panel}</SettingsPanel>
        </VStack>
      ) : (
        <Layout
          height="auto"
          contentWidth={1000}
          content={
            <LayoutContent padding={6}>
              <VStack gap={6}>
                <VStack gap={3}>
                  {controller.error ? (
                    <Banner
                      status="error"
                      title="Could not save that change"
                      description={controller.error}
                      isDismissable
                      onDismiss={controller.clearError}
                    />
                  ) : null}
                  {overview.active_kids ? (
                    <Banner
                      status="warning"
                      title={`Kids mode is active for ${overview.active_kids.name}`}
                      description="Leave kids mode on the TV with the guardian PIN to manage another profile."
                    />
                  ) : null}
                </VStack>
                <Section padding={0}>
                  <VStack gap={0}>
                    <Toolbar
                      label="Profiles"
                      startContent={
                        <VStack gap={0}>
                          <Heading level={2}>Who&rsquo;s watching?</Heading>
                          <Text type="supporting" color="secondary">
                            Settings belong to a profile. Its PIN also protects access here.
                          </Text>
                        </VStack>
                      }
                      endContent={
                        <Button
                          label="New profile"
                          variant="primary"
                          isDisabled={!canCreate}
                          onClick={() => setCreating(true)}
                        />
                      }
                      dividers={['bottom']}
                    />
                    <VStack gap={4} padding={4}>
                      <Grid columns={{minWidth: 220, repeat: 'fit'}} gap={3}>
                        {profiles.map((profile) => (
                          <ClickableCard
                            key={profile.id}
                            label={`Open ${profile.name}`}
                            variant={selected?.id === profile.id ? 'green' : 'default'}
                            elevation="low"
                            onClick={() => openProfile(profile)}
                          >
                            <HStack gap={3} align="center">
                              <Avatar name={profile.name} size="lg" shape="rounded" />
                              <VStack gap={0}>
                                <Text weight="semibold">{profile.name}</Text>
                                <Text type="supporting" color="secondary">
                                  {profileSubtitle(profile)}
                                </Text>
                              </VStack>
                            </HStack>
                          </ClickableCard>
                        ))}
                      </Grid>
                      <Text type="supporting" color="secondary">
                        {selected
                          ? `Managing ${selected.name}. Playback on the TV is unchanged.`
                          : 'Choose a profile to manage it. The remote floats in the corner.'}
                      </Text>
                    </VStack>
                  </VStack>
                </Section>
              </VStack>
            </LayoutContent>
          }
        />
      )}

      <SelectProfileDialog
        profile={selecting}
        busy={controller.busy}
        onClose={() => setSelecting(null)}
        onSubmit={(pin) => {
          const profile = selecting;
          if (!profile) return;
          controller
            .selectProfile(profile.id, pin)
            .then(() => {
              setSelecting(null);
              navigate('/manage/addons');
            })
            .catch(() => {});
        }}
      />
      <AddonDialog
        addon={configuring}
        open={installing || configuring !== null}
        busy={controller.busy}
        onClose={() => {
          setInstalling(false);
          setConfiguring(null);
        }}
        onSubmit={(url, allowLocal) => {
          const target = configuring;
          const action = target
            ? controller.configureAddon(target.installation_id, url, allowLocal)
            : controller.installAddon(url, allowLocal);
          action
            .then(() => {
              setInstalling(false);
              setConfiguring(null);
            })
            .catch(() => {});
        }}
      />
      <ShareDialog
        addon={sharing}
        targets={targets}
        open={sharing !== null}
        busy={controller.busy}
        onClose={() => setSharing(null)}
        onSubmit={(targetId, pin) => {
          const addon = sharing;
          if (!addon) return;
          controller
            .shareAddon(addon.installation_id, targetId, pin)
            .then(() => setSharing(null))
            .catch(() => {});
        }}
      />
      <ConfirmDialog
        open={removing !== null}
        title="Remove addon?"
        body={
          removing
            ? `Remove ${removing.manifest.name} from ${selected?.name ?? 'this profile'}? Other linked profiles keep their installation.`
            : ''
        }
        confirmLabel="Remove addon"
        busy={controller.busy}
        onClose={() => setRemoving(null)}
        onConfirm={() => {
          const addon = removing;
          if (!addon) return;
          controller
            .removeAddon(addon.installation_id)
            .then(() => setRemoving(null))
            .catch(() => {});
        }}
      />
      <NewProfileDialog
        open={creating}
        hasGuardian={canCreate}
        busy={controller.busy}
        onClose={() => setCreating(false)}
        onSubmit={(name, pin, kids) => {
          controller
            .createProfile(name, pin, kids)
            .then(() => setCreating(false))
            .catch(() => {});
        }}
      />
    </AppShell>
  );
}
