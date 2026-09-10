import {Button} from '@astryxdesign/core/Button';
import {EmptyState} from '@astryxdesign/core/EmptyState';
import {HStack} from '@astryxdesign/core/HStack';
import {Icon} from '@astryxdesign/core/Icon';
import {IconButton} from '@astryxdesign/core/IconButton';
import {MoreMenu} from '@astryxdesign/core/MoreMenu';
import {Switch} from '@astryxdesign/core/Switch';
import {Token} from '@astryxdesign/core/Token';
import {VStack} from '@astryxdesign/core/VStack';
import {ArrowDownIcon, ArrowUpIcon, PuzzlePieceIcon} from '@heroicons/react/24/outline';
import type {AddonSummary} from '../client';
import {SettingsCard, SettingsRow} from './settings';

/** Addons are records, so they render as rows in one group rather than as cards. */
export function AddonsPanel({
  addons,
  busy,
  disabled,
  onInstall,
  onToggle,
  onMove,
  onConfigure,
  onShare,
  onRemove,
}: {
  addons: AddonSummary[];
  busy: boolean;
  disabled: boolean;
  onInstall: () => void;
  onToggle: (addon: AddonSummary) => void;
  onMove: (index: number, delta: number) => void;
  onConfigure: (addon: AddonSummary) => void;
  onShare: (addon: AddonSummary) => void;
  onRemove: (addon: AddonSummary) => void;
}) {
  const blocked = busy || disabled;
  if (addons.length === 0) {
    return (
      <VStack padding={4}>
        <EmptyState
          title="No addons yet"
          description="Paste a configured manifest URL from your addon provider to bring in movies and series."
          actions={<Button label="Install addon" variant="primary" isDisabled={disabled} onClick={onInstall} />}
        />
      </VStack>
    );
  }
  return (
    <SettingsCard title="Installed addons">
      {addons.map((addon, index) => {
        const description =
          typeof addon.manifest.description === 'string' && addon.manifest.description
            ? `${addon.manifest.description}`
            : `Version ${addon.manifest.version}`;
        return (
          <SettingsRow
            key={addon.installation_id}
            title={addon.manifest.name}
            titleAccessory={index === 0 ? <Token label="First" size="sm" /> : undefined}
            description={
              addon.allow_local ? `${description} · Allowed on your local network` : description
            }
            icon={PuzzlePieceIcon}
            control={
              <HStack gap={1} align="center">
                <IconButton
                  label={`Move ${addon.manifest.name} up`}
                  icon={<Icon icon={ArrowUpIcon} size="sm" />}
                  variant="ghost"
                  size="sm"
                  isDisabled={blocked || index === 0}
                  onClick={() => onMove(index, -1)}
                />
                <IconButton
                  label={`Move ${addon.manifest.name} down`}
                  icon={<Icon icon={ArrowDownIcon} size="sm" />}
                  variant="ghost"
                  size="sm"
                  isDisabled={blocked || index === addons.length - 1}
                  onClick={() => onMove(index, 1)}
                />
                <Switch
                  label={`Enable ${addon.manifest.name}`}
                  isLabelHidden
                  size="sm"
                  value={addon.enabled}
                  isDisabled={blocked}
                  onChange={() => onToggle(addon)}
                />
                <MoreMenu
                  label={`More actions for ${addon.manifest.name}`}
                  variant="ghost"
                  size="sm"
                  items={[
                    {
                      id: 'configure',
                      label: 'Reconfigure',
                      icon: <Icon icon="wrench" size="sm" />,
                      isDisabled: blocked,
                      onClick: () => onConfigure(addon),
                    },
                    {
                      id: 'share',
                      label: 'Share with another profile',
                      isDisabled: blocked,
                      onClick: () => onShare(addon),
                    },
                    {type: 'divider'},
                    {
                      id: 'remove',
                      label: 'Remove addon',
                      variant: 'destructive',
                      isDisabled: blocked,
                      onClick: () => onRemove(addon),
                    },
                  ]}
                />
              </HStack>
            }
          />
        );
      })}
    </SettingsCard>
  );
}
