'use client'

import { useRef } from 'react'
import { useTranslations } from 'next-intl'
import { MaximizeIcon, VideoIcon } from 'lucide-react'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog'

import { apiIdSegment } from './api-path'
import type { Printer } from './dashboard-types'

export function CameraDialogControl({ printer }: { printer: Printer }) {
  const t = useTranslations('inventory')
  const frameRef = useRef<HTMLDivElement>(null)

  return (
    <Dialog>
      <DialogTrigger
        render={
          <Button
            className="min-h-8 w-full gap-2 rounded-md bg-muted/50 px-3 py-2 hover:bg-muted dark:hover:bg-muted"
            type="button"
            variant="ghost"
          />
        }
      >
        <VideoIcon />
        {t('viewCamera')}
      </DialogTrigger>
      <DialogContent className="sm:max-w-3xl" closeLabel={t('closeCamera')}>
        <DialogHeader>
          <DialogTitle>{t('cameraTitle')}</DialogTitle>
        </DialogHeader>
        <div ref={frameRef} className="relative overflow-hidden rounded-md bg-black">
          <video
            aria-label={t('cameraTitle')}
            autoPlay
            className="aspect-video w-full bg-black object-contain"
            muted
            playsInline
            src={`/api/tenants/${apiIdSegment(printer.tenant_id, 'tenant_id')}/printers/${apiIdSegment(printer.id, 'printer_id')}/camera.mp4`}
          />
          <Button
            aria-label={t('cameraFullscreen')}
            className="absolute right-3 top-3 rounded-md bg-black/70 text-white hover:bg-black/90 hover:text-white dark:hover:bg-black/90"
            onClick={() => void frameRef.current?.requestFullscreen()}
            size="icon"
            title={t('cameraFullscreen')}
            type="button"
            variant="ghost"
          >
            <MaximizeIcon className="size-4" />
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
