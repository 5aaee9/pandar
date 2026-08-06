'use client'

import { useEffect, useRef, useState } from 'react'
import { useTranslations } from 'next-intl'
import { MaximizeIcon, PictureInPicture2Icon, VideoIcon } from 'lucide-react'

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
  const videoRef = useRef<HTMLVideoElement>(null)
  const [open, setOpen] = useState(false)
  const [videoMounted, setVideoMounted] = useState(false)
  const pictureInPictureSupported =
    videoMounted &&
    document.pictureInPictureEnabled &&
    typeof HTMLVideoElement.prototype.requestPictureInPicture === 'function'

  useEffect(() => {
    if (!videoMounted) return

    const video = videoRef.current
    const handleLeavePictureInPicture = () => {
      if (!open) setVideoMounted(false)
    }
    video?.addEventListener('leavepictureinpicture', handleLeavePictureInPicture)
    return () => video?.removeEventListener('leavepictureinpicture', handleLeavePictureInPicture)
  }, [open, videoMounted])

  const handleOpenChange = (nextOpen: boolean) => {
    setOpen(nextOpen)
    setVideoMounted(nextOpen || document.pictureInPictureElement === videoRef.current)
  }

  const enterPictureInPicture = async () => {
    await videoRef.current?.requestPictureInPicture()
    setOpen(false)
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
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
      {videoMounted ? (
        <DialogContent className="sm:max-w-3xl" closeLabel={t('closeCamera')} keepMounted>
          <DialogHeader>
            <DialogTitle>{t('cameraTitle')}</DialogTitle>
          </DialogHeader>
          <div ref={frameRef} className="relative overflow-hidden rounded-md bg-black">
            <video
              ref={videoRef}
              aria-label={t('cameraTitle')}
              autoPlay
              className="aspect-video w-full bg-black object-contain"
              muted
              playsInline
              src={`/api/tenants/${apiIdSegment(printer.tenant_id, 'tenant_id')}/printers/${apiIdSegment(printer.id, 'printer_id')}/camera.mp4`}
            />
            <div className="absolute right-3 top-3 flex gap-2">
              {pictureInPictureSupported ? (
                <Button
                  aria-label={t('cameraPictureInPicture')}
                  className="rounded-md bg-black/70 text-white hover:bg-black/90 hover:text-white dark:hover:bg-black/90"
                  onClick={() => void enterPictureInPicture()}
                  size="icon"
                  title={t('cameraPictureInPicture')}
                  type="button"
                  variant="ghost"
                >
                  <PictureInPicture2Icon className="size-4" />
                </Button>
              ) : null}
              <Button
                aria-label={t('cameraFullscreen')}
                className="rounded-md bg-black/70 text-white hover:bg-black/90 hover:text-white dark:hover:bg-black/90"
                onClick={() => void frameRef.current?.requestFullscreen()}
                size="icon"
                title={t('cameraFullscreen')}
                type="button"
                variant="ghost"
              >
                <MaximizeIcon className="size-4" />
              </Button>
            </div>
          </div>
        </DialogContent>
      ) : null}
    </Dialog>
  )
}
